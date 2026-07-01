//! C ABI exports for rememhq-core, consumed by Python cffi, Swift, and Node.js native bindings.

#![allow(clippy::missing_safety_doc)]

use crate::config::RememConfig;
use crate::memory::types::{ForgetMode, MemoryRecord, MemoryType};

use crate::reasoning::ReasoningEngine;
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::{HNSWVectorIndex, VectorIndex};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float};
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;
use uuid::Uuid;

/// Opaque wrapper for `Arc<ReasoningEngine>` passed across the FFI boundary
pub struct RememEngine {
    pub engine: Arc<ReasoningEngine>,
}

fn get_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime for remem FFI")
    })
}

unsafe fn set_error(out_error: *mut *mut c_char, err: anyhow::Error) {
    if !out_error.is_null() {
        let err_str = err.to_string();
        let c_str =
            CString::new(err_str).unwrap_or_else(|_| CString::new("Unknown FFI error").unwrap());
        *out_error = c_str.into_raw();
    }
}

fn alloc_cstring(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new("{}").unwrap())
        .into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn remem_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_engine_new(
    project: *const c_char,
    data_dir: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut std::ffi::c_void {
    if project.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null project string passed to remem_engine_new"),
            );
        }
        return std::ptr::null_mut();
    }

    let project_str = unsafe { CStr::from_ptr(project) }
        .to_string_lossy()
        .to_string();
    let data_dir_path = if data_dir.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(data_dir) }
            .to_string_lossy()
            .to_string();
        Some(std::path::PathBuf::from(s))
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            let config = RememConfig::load(&project_str, data_dir_path.as_deref())?;
            let store = Arc::new(SqliteStore::open(&config.db_path())?);

            let target_provider = config.reasoning.provider.clone();
            let _ = target_provider; // consumed by logging below

            let provider = crate::providers::factory::build_reasoning_provider(&config);
            let embeddings = crate::providers::factory::build_embedding_provider(&config);

            let index = Arc::new(HNSWVectorIndex::new(embeddings.dimension(), 10000));
            let _ = index.load(&config.index_path()).await;

            let engine =
                ReasoningEngine::new(config, provider, embeddings, store, index, Vec::new());
            Ok(RememEngine {
                engine: Arc::new(engine),
            })
        })
    }));

    match result {
        Ok(Ok(engine)) => Box::into_raw(Box::new(engine)) as *mut std::ffi::c_void,
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(
                    out_error,
                    anyhow::anyhow!("Panic during engine initialization"),
                );
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_engine_free(engine: *mut std::ffi::c_void) {
    if !engine.is_null() {
        unsafe {
            let _ = Box::from_raw(engine as *mut RememEngine);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_store(
    engine: *mut std::ffi::c_void,
    content: *const c_char,
    tags_json: *const c_char,
    importance: f32,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || content.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_store"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let content_str = unsafe { CStr::from_ptr(content) }
        .to_string_lossy()
        .to_string();

    let tags: Vec<String> = if tags_json.is_null() {
        vec![]
    } else {
        let t_json = unsafe { CStr::from_ptr(tags_json) }.to_bytes();
        serde_json::from_slice(t_json).unwrap_or_default()
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            let auto_score = importance < 0.0;
            let mut record = MemoryRecord::new(&content_str, MemoryType::Fact).with_tags(tags);
            if !auto_score {
                record = record.with_importance(importance);
            }
            wrapper.engine.store_memory(record, auto_score, None).await
        })
    }));

    match result {
        Ok(Ok(record)) => {
            let json = serde_json::to_string(&record).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_store"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_recall(
    engine: *mut std::ffi::c_void,
    query: *const c_char,
    limit: usize,
    filter_tags_json: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || query.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_recall"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let query_str = unsafe { CStr::from_ptr(query) }
        .to_string_lossy()
        .to_string();

    let tags: Vec<String> = if filter_tags_json.is_null() {
        vec![]
    } else {
        let t_json = unsafe { CStr::from_ptr(filter_tags_json) }.to_bytes();
        serde_json::from_slice(t_json).unwrap_or_default()
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            wrapper
                .engine
                .recall(&query_str, limit, &tags, None, None, None)
                .await
        })
    }));

    match result {
        Ok(Ok(results)) => {
            let json = serde_json::to_string(&results).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_recall"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_search(
    engine: *mut std::ffi::c_void,
    query: *const c_char,
    limit: usize,
    filter_tags_json: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || query.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_search"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let query_str = unsafe { CStr::from_ptr(query) }
        .to_string_lossy()
        .to_string();

    let tags: Vec<String> = if filter_tags_json.is_null() {
        vec![]
    } else {
        let t_json = unsafe { CStr::from_ptr(filter_tags_json) }.to_bytes();
        serde_json::from_slice(t_json).unwrap_or_default()
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.search(&query_str, limit, &tags, None).await })
    }));

    match result {
        Ok(Ok(results)) => {
            let json = serde_json::to_string(&results).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_search"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_update(
    engine: *mut std::ffi::c_void,
    id_str: *const c_char,
    content: *const c_char,
    importance: c_float,
    tags_json: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || id_str.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_update"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_string = unsafe { CStr::from_ptr(id_str) }
        .to_string_lossy()
        .to_string();
    let id = match Uuid::parse_str(&id_string) {
        Ok(u) => u,
        Err(e) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Invalid UUID format: {}", e));
            }
            return std::ptr::null_mut();
        }
    };

    let content_opt = if content.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(content) }
                .to_string_lossy()
                .to_string(),
        )
    };

    let importance_opt = if importance < 0.0 {
        None
    } else {
        Some(importance)
    };

    let tags_opt: Option<Vec<String>> = if tags_json.is_null() {
        None
    } else {
        let t_json = unsafe { CStr::from_ptr(tags_json) }.to_bytes();
        serde_json::from_slice(t_json).ok()
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            wrapper
                .engine
                .update_memory(id, content_opt, importance_opt, tags_opt, None)
                .await
        })
    }));

    match result {
        Ok(Ok(record)) => {
            let json = serde_json::to_string(&record).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_update"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_forget(
    engine: *mut std::ffi::c_void,
    id_str: *const c_char,
    mode_str: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    if engine.is_null() || id_str.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_forget"),
            );
        }
        return false;
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_string = unsafe { CStr::from_ptr(id_str) }
        .to_string_lossy()
        .to_string();
    let id = match Uuid::parse_str(&id_string) {
        Ok(u) => u,
        Err(e) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Invalid UUID format: {}", e));
            }
            return false;
        }
    };

    let mode = if mode_str.is_null() {
        ForgetMode::Delete
    } else {
        let m_str = unsafe { CStr::from_ptr(mode_str) }
            .to_string_lossy()
            .to_lowercase();
        match m_str.as_str() {
            "archive" => ForgetMode::Archive,
            "decay" => ForgetMode::Decay,
            _ => ForgetMode::Delete,
        }
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.forget(id, mode).await })
    }));

    match result {
        Ok(Ok(success)) => success,
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            false
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_forget"));
            }
            false
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_decay(
    engine: *mut std::ffi::c_void,
    factor: f32,
    out_error: *mut *mut c_char,
) -> i32 {
    if engine.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_decay"),
            );
        }
        return -1;
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.apply_decay(factor).await })
    }));

    match result {
        Ok(Ok(count)) => count as i32,
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            -1
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_decay"));
            }
            -1
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_query_knowledge(
    engine: *mut std::ffi::c_void,
    subject: *const c_char,
    predicate: *const c_char,
    object: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_query_knowledge"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };

    let s_opt = if subject.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(subject) }
                .to_str()
                .unwrap_or_default(),
        )
    };
    let p_opt = if predicate.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(predicate) }
                .to_str()
                .unwrap_or_default(),
        )
    };
    let o_opt = if object.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(object) }
                .to_str()
                .unwrap_or_default(),
        )
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.query_knowledge(s_opt, p_opt, o_opt).await })
    }));

    match result {
        Ok(Ok(triples)) => {
            let json = serde_json::to_string(&triples).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_query_knowledge"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_get_entity_context(
    engine: *mut std::ffi::c_void,
    entity: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || entity.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_get_entity_context"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let entity_str = unsafe { CStr::from_ptr(entity) }
        .to_string_lossy()
        .to_string();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.get_entity_context(&entity_str).await })
    }));

    match result {
        Ok(Ok(triples)) => {
            let json = serde_json::to_string(&triples).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(
                    out_error,
                    anyhow::anyhow!("Panic in remem_get_entity_context"),
                );
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_session_create(
    engine: *mut std::ffi::c_void,
    session_id: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    if engine.is_null() || session_id.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_session_create"),
            );
        }
        return false;
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_str = unsafe { CStr::from_ptr(session_id) }
        .to_string_lossy()
        .to_string();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.create_session(&id_str).await })
    }));

    match result {
        Ok(Ok(())) => true,
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            false
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_session_create"));
            }
            false
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_session_end(
    engine: *mut std::ffi::c_void,
    session_id: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    if engine.is_null() || session_id.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_session_end"),
            );
        }
        return false;
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_str = unsafe { CStr::from_ptr(session_id) }
        .to_string_lossy()
        .to_string();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.end_session(&id_str).await })
    }));

    match result {
        Ok(Ok(found)) => found,
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            false
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_session_end"));
            }
            false
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_session_get(
    engine: *mut std::ffi::c_void,
    session_id: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || session_id.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_session_get"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_str = unsafe { CStr::from_ptr(session_id) }
        .to_string_lossy()
        .to_string();

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.get_session(&id_str).await })
    }));

    match result {
        // Mirror the null-pointer-with-no-error convention used by
        // remem_query_knowledge/remem_get_entity_context for "found nothing":
        // a missing session is not itself an error.
        Ok(Ok(None)) => std::ptr::null_mut(),
        Ok(Ok(Some(session))) => {
            let json = serde_json::to_string(&session).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_session_get"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_session_list(
    engine: *mut std::ffi::c_void,
    limit: usize,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_session_list"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { wrapper.engine.list_sessions(limit).await })
    }));

    match result {
        Ok(Ok(sessions)) => {
            let json = serde_json::to_string(&sessions).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_session_list"));
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn remem_consolidate(
    engine: *mut std::ffi::c_void,
    session_id: *const c_char,
    model: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    if engine.is_null() || session_id.is_null() {
        unsafe {
            set_error(
                out_error,
                anyhow::anyhow!("Null pointer passed to remem_consolidate"),
            );
        }
        return std::ptr::null_mut();
    }

    let wrapper = unsafe { &*(engine as *mut RememEngine) };
    let id_str = unsafe { CStr::from_ptr(session_id) }
        .to_string_lossy()
        .to_string();
    let model_str = if model.is_null() {
        wrapper.engine.config.reasoning.reasoning_model.clone()
    } else {
        unsafe { CStr::from_ptr(model) }
            .to_string_lossy()
            .to_string()
    };

    let rt = get_runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async {
            crate::reasoning::consolidation::consolidate_session(
                &*wrapper.engine.provider,
                &*wrapper.engine.embeddings,
                &wrapper.engine.store,
                wrapper.engine.index.as_ref(),
                &id_str,
                &model_str,
                None,
            )
            .await
        })
    }));

    match result {
        Ok(Ok(report)) => {
            let json = serde_json::to_string(&report).unwrap_or_default();
            alloc_cstring(json)
        }
        Ok(Err(err)) => {
            unsafe {
                set_error(out_error, err);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe {
                set_error(out_error, anyhow::anyhow!("Panic in remem_consolidate"));
            }
            std::ptr::null_mut()
        }
    }
}
