//! JNI bridge for the Android side of the React Native binding.
//!
//! This mirrors `rememhq-core/src/ffi/mod.rs`'s C ABI surface
//! method-for-method, but talks to `rememhq-core`'s Rust API directly
//! (`ReasoningEngine`, `providers::factory`, `RememConfig`) rather than
//! going through that C ABI — there's no reason to round-trip through a
//! second JSON (de)serialization pass when this crate can depend on
//! rememhq-core's real Rust types directly.
//!
//! Naming convention: every exported function is
//! `Java_io_remem_expo_NativeBridge_<method>`, matching JNI's mangled
//! name for a native method on `io.remem.expo.NativeBridge` (see
//! `android/src/main/java/io/remem/expo/NativeBridge.kt`). The handle
//! returned by `openEngine` is a `jlong` — really a boxed
//! `Arc<ReasoningEngine>` pointer, owned by the Kotlin side until it
//! calls `closeEngine`.
//!
//! Error handling: every fallible function throws a Java
//! `java.lang.RuntimeException` via `env.throw_new(...)` on failure,
//! rather than returning a sentinel value, since JNI makes "did this
//! call fail" a first-class concept (`env.exception_check()` on the
//! Kotlin side after every call) rather than something to encode in the
//! return value the way the C ABI's null-pointer/out-param convention
//! does.
//!
//! Async: every rememhq-core engine method is a Tokio future. JNI calls
//! are fundamentally synchronous (the calling Kotlin thread blocks until
//! the native function returns), so each function here gets a
//! lazily-initialized, shared multi-threaded Tokio runtime and blocks on
//! it — the same pattern `rememhq-core/src/ffi/mod.rs`'s `get_runtime()`
//! uses, just not literally shared with it (this crate can't reach that
//! private function from outside rememhq-core, and a second runtime
//! instance is the correct, intentional outcome here — see this crate's
//! own Cargo.toml for why this isn't a workspace member).

use std::sync::{Arc, OnceLock};

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use tokio::runtime::Runtime;

use rememhq_core::config::RememConfig;
use rememhq_core::memory::types::ForgetMode;
use rememhq_core::providers::factory;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};

fn get_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime for remem Android JNI bridge")
    })
}

/// Throws a Java `RuntimeException` carrying `message`. If a Java
/// exception is already pending (this shouldn't normally happen, since
/// we check `exception_check()` isn't needed here — we never call back
/// into JNI methods that themselves throw before this point — but it's
/// cheap insurance), this silently does nothing rather than risk a
/// double-throw panic, matching `jni` crate's own documented advice.
fn throw_runtime_exception(env: &mut JNIEnv, message: &str) {
    if env.exception_check().unwrap_or(false) {
        return;
    }
    let _ = env.throw_new("java/lang/RuntimeException", message);
}

/// Converts a `jlong` handle back into the `Arc<ReasoningEngine>` it
/// represents, without taking ownership (the handle stays valid for
/// further calls — only `closeEngine` actually drops it). Returns `None`
/// (after throwing) if the handle is `0` (Kotlin's sentinel for "no
/// engine"/already closed).
///
/// # Safety
/// The caller must guarantee `handle` was produced by `openEngine` and
/// has not since been passed to `closeEngine`. This mirrors the same
/// trust boundary `rememhq-core`'s C ABI has for its `*mut c_void` engine
/// pointers — the native layer cannot itself verify a handle's validity
/// beyond the null check.
unsafe fn engine_from_handle(
    env: &mut JNIEnv,
    handle: jlong,
) -> Option<Arc<ReasoningEngine>> {
    if handle == 0 {
        throw_runtime_exception(env, "Engine handle is 0 (already closed, or never opened)");
        return None;
    }
    // Reconstruct without dropping: wrap in Arc, clone out a new owned
    // reference for the caller, then immediately forget the temporary
    // Arc so the original strong-count-owning allocation (held by
    // Kotlin's jlong) isn't decremented.
    let ptr = handle as *const ReasoningEngine;
    let temp = Arc::from_raw(ptr);
    let cloned = Arc::clone(&temp);
    std::mem::forget(temp);
    Some(cloned)
}

/// Reads a (possibly null) Java string into an `Option<String>`. Returns
/// `None` for Java `null`, matching the C ABI's null-pointer-as-absent
/// convention used throughout `rememhq-core/include/rememhq.h`.
fn read_optional_string(env: &mut JNIEnv, value: &JString) -> Option<String> {
    if value.is_null() {
        return None;
    }
    env.get_string(value).ok().map(|s| s.into())
}

fn read_required_string(env: &mut JNIEnv, value: &JString) -> anyhow::Result<String> {
    env.get_string(value)
        .map(|s| s.into())
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8/UTF-16 string from JVM: {e}"))
}

fn new_jstring(env: &mut JNIEnv, value: &str) -> jstring {
    env.new_string(value)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_openEngine(
    mut env: JNIEnv,
    _class: JClass,
    project: JString,
    data_dir: JString,
) -> jlong {
    let project_str = match read_required_string(&mut env, &project) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return 0;
        }
    };
    let data_dir_str = read_optional_string(&mut env, &data_dir);

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            let config = RememConfig::load(&project_str, data_dir_str.as_ref().map(std::path::Path::new))?;
            let provider = factory::build_reasoning_provider(&config);
            let embeddings = factory::build_embedding_provider(&config);
            let store = Arc::new(SqliteStore::open(&config.db_path())?);
            let index: Arc<dyn VectorIndex> = Arc::new(HNSWVectorIndex::new(
                embeddings.dimension(),
                100_000,
            ));
            if config.index_path().exists() {
                let _ = index.load(&config.index_path()).await;
            }
            anyhow::Ok(ReasoningEngine::new(config, provider, embeddings, store, index))
        })
    }));

    match result {
        Ok(Ok(engine)) => {
            let arc = Arc::new(engine);
            Arc::into_raw(arc) as jlong
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            0
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic while opening remem engine");
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_closeEngine(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    // SAFETY: dropping the Arc this handle represents. Caller (Kotlin)
    // must not use this handle again after this call — same contract as
    // remem_engine_free in the C ABI.
    unsafe {
        let _ = Arc::from_raw(handle as *const ReasoningEngine);
    }
}

// ---------------------------------------------------------------------------
// Memory operations
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_store(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    content: JString,
    tags_json: JString,
    importance: jfloat,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let content_str = match read_required_string(&mut env, &content) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };
    let tags: Vec<String> = read_optional_string(&mut env, &tags_json)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let auto_score = importance < 0.0;

    let mut record = rememhq_core::MemoryRecord::new(&content_str, rememhq_core::MemoryType::Fact)
        .with_tags(tags);
    if !auto_score {
        record = record.with_importance(importance);
    }

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.store_memory(record, auto_score).await })
    }));

    match result {
        Ok(Ok(stored_record)) => {
            let json = serde_json::to_string(&stored_record).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in store");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_recall(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    query: JString,
    limit: jint,
    filter_tags_json: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let query_str = match read_required_string(&mut env, &query) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };
    let tags: Vec<String> = read_optional_string(&mut env, &filter_tags_json)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.recall(&query_str, limit.max(0) as usize, &tags, None, None).await })
    }));

    match result {
        Ok(Ok(results)) => {
            let json = serde_json::to_string(&results).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in recall");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_search(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    query: JString,
    limit: jint,
    filter_tags_json: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let query_str = match read_required_string(&mut env, &query) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };
    let tags: Vec<String> = read_optional_string(&mut env, &filter_tags_json)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.search(&query_str, limit.max(0) as usize, &tags).await })
    }));

    match result {
        Ok(Ok(results)) => {
            let json = serde_json::to_string(&results).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in search");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_update(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
    content: JString,
    importance: jfloat,
    tags_json: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let id_str = match read_required_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };
    let parsed_id = match uuid::Uuid::parse_str(&id_str) {
        Ok(u) => u,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("Invalid memory id '{id_str}': {e}"));
            return std::ptr::null_mut();
        }
    };
    let content_opt = read_optional_string(&mut env, &content);
    let importance_opt = if importance < 0.0 { None } else { Some(importance) };
    let tags_opt: Option<Vec<String>> = read_optional_string(&mut env, &tags_json)
        .and_then(|json| serde_json::from_str(&json).ok());

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            engine
                .update_memory(parsed_id, content_opt, importance_opt, tags_opt)
                .await
        })
    }));

    match result {
        Ok(Ok(record)) => {
            let json = serde_json::to_string(&record).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in update");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_forget(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
    mode: JString,
) -> jboolean {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return JNI_FALSE,
    };
    let id_str = match read_required_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return JNI_FALSE;
        }
    };
    let parsed_id = match uuid::Uuid::parse_str(&id_str) {
        Ok(u) => u,
        Err(e) => {
            throw_runtime_exception(&mut env, &format!("Invalid memory id '{id_str}': {e}"));
            return JNI_FALSE;
        }
    };
    let mode_str = read_optional_string(&mut env, &mode).unwrap_or_else(|| "delete".to_string());
    let forget_mode = match mode_str.to_lowercase().as_str() {
        "delete" => ForgetMode::Delete,
        "decay" => ForgetMode::Decay,
        "archive" => ForgetMode::Archive,
        other => {
            throw_runtime_exception(&mut env, &format!("Unknown forget mode: '{other}'"));
            return JNI_FALSE;
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.forget(parsed_id, forget_mode).await })
    }));

    match result {
        Ok(Ok(found)) => {
            if found {
                JNI_TRUE
            } else {
                JNI_FALSE
            }
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            JNI_FALSE
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in forget");
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_decay(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    factor: jfloat,
) -> jint {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return -1,
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.apply_decay(factor).await })
    }));

    match result {
        Ok(Ok(count)) => count as jint,
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            -1
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in decay");
            -1
        }
    }
}

// ---------------------------------------------------------------------------
// Knowledge graph
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_queryKnowledge(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    subject: JString,
    predicate: JString,
    object: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let subject_opt = read_optional_string(&mut env, &subject);
    let predicate_opt = read_optional_string(&mut env, &predicate);
    let object_opt = read_optional_string(&mut env, &object);

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            engine
                .query_knowledge(
                    subject_opt.as_deref(),
                    predicate_opt.as_deref(),
                    object_opt.as_deref(),
                )
                .await
        })
    }));

    match result {
        Ok(Ok(triples)) => {
            let json = serde_json::to_string(&triples).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in queryKnowledge");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_entityContext(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    entity: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let entity_str = match read_required_string(&mut env, &entity) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.get_entity_context(&entity_str).await })
    }));

    match result {
        Ok(Ok(triples)) => {
            let json = serde_json::to_string(&triples).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in entityContext");
            std::ptr::null_mut()
        }
    }
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_startSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return,
    };
    let id_str = match read_required_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return;
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.create_session(&id_str).await })
    }));

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => throw_runtime_exception(&mut env, &err.to_string()),
        Err(_) => throw_runtime_exception(&mut env, "Panic in startSession"),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_endSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) -> jboolean {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return JNI_FALSE,
    };
    let id_str = match read_required_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return JNI_FALSE;
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.end_session(&id_str).await })
    }));

    match result {
        Ok(Ok(found)) => {
            if found {
                JNI_TRUE
            } else {
                JNI_FALSE
            }
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            JNI_FALSE
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in endSession");
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_getSession(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let id_str = match read_required_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.get_session(&id_str).await })
    }));

    match result {
        // Mirrors the C ABI's convention: a missing session returns
        // null without throwing, since "not found" isn't itself an error.
        Ok(Ok(None)) => std::ptr::null_mut(),
        Ok(Ok(Some(session))) => {
            let json = serde_json::to_string(&session).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in getSession");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_listSessions(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    limit: jint,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { engine.list_sessions(limit.max(0) as usize).await })
    }));

    match result {
        Ok(Ok(sessions)) => {
            let json = serde_json::to_string(&sessions).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in listSessions");
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_remem_expo_NativeBridge_consolidate(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    session_id: JString,
    model: JString,
) -> jstring {
    let engine = match unsafe { engine_from_handle(&mut env, handle) } {
        Some(e) => e,
        None => return std::ptr::null_mut(),
    };
    let session_id_str = match read_required_string(&mut env, &session_id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime_exception(&mut env, &e.to_string());
            return std::ptr::null_mut();
        }
    };
    let model_str = read_optional_string(&mut env, &model)
        .unwrap_or_else(|| engine.config.reasoning.reasoning_model.clone());

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            rememhq_core::reasoning::consolidation::consolidate_session(
                &*engine.provider,
                &*engine.embeddings,
                &engine.store,
                engine.index.as_ref(),
                &session_id_str,
                &model_str,
            )
            .await
        })
    }));

    match result {
        Ok(Ok(report)) => {
            let json = serde_json::to_string(&report).unwrap_or_default();
            new_jstring(&mut env, &json)
        }
        Ok(Err(err)) => {
            throw_runtime_exception(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        Err(_) => {
            throw_runtime_exception(&mut env, "Panic in consolidate");
            std::ptr::null_mut()
        }
    }
}
