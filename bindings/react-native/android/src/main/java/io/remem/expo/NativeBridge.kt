package io.remem.expo

/**
 * Thin JNI declarations matching `bindings/react-native/android/rust/src/lib.rs`
 * function-for-function. Every `external fun` here must have a name and
 * signature matching one `Java_io_remem_expo_NativeBridge_<name>` export
 * in that file exactly — JNI resolves native methods by mangled name and
 * does NOT type-check signatures at compile time, so a mismatch here is
 * a runtime `UnsatisfiedLinkError`, not a build error.
 *
 * Conventions mirrored from the Rust side (see lib.rs's own doc comment
 * for the full rationale):
 * - `engineId` is an opaque `Long` handle (really a boxed `Arc` pointer)
 *   returned by [openEngine]. `0L` is never a valid handle.
 * - Nullable `String?` parameters use Kotlin's native null bridging —
 *   JNI sees a real `null` jstring, matching the C ABI's
 *   null-pointer-as-absent convention.
 * - `importance` uses `-1f` as the "auto-score" (store) / "leave
 *   unchanged" (update) sentinel, matching `rememhq.h`'s own convention
 *   for the same parameter.
 * - Every function throws `java.lang.RuntimeException` on failure rather
 *   than returning a sentinel — callers should expect normal Kotlin
 *   exception handling, not null-checking, for error cases.
 *
 * JSON in, JSON out: every `String?` return value here is raw JSON
 * (object or array), matching exactly what `rememhq-core`'s engine
 * methods serialize to. [RememModule] is responsible for converting
 * these into the `Bundle`/`Map` shapes Expo's bridge can carry to JS —
 * this object does no JSON parsing of its own.
 */
internal object NativeBridge {
  init {
    System.loadLibrary("remem_android_jni")
  }

  // ---------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------

  external fun openEngine(project: String, dataDir: String?): Long

  external fun closeEngine(engineId: Long)

  // ---------------------------------------------------------------------
  // Memory operations
  // ---------------------------------------------------------------------

  external fun store(
    engineId: Long,
    content: String,
    tagsJson: String?,
    importance: Float,
  ): String

  external fun recall(
    engineId: Long,
    query: String,
    limit: Int,
    filterTagsJson: String?,
  ): String

  external fun search(
    engineId: Long,
    query: String,
    limit: Int,
    filterTagsJson: String?,
  ): String

  external fun update(
    engineId: Long,
    id: String,
    content: String?,
    importance: Float,
    tagsJson: String?,
  ): String

  external fun forget(engineId: Long, id: String, mode: String): Boolean

  external fun decay(engineId: Long, factor: Float): Int

  // ---------------------------------------------------------------------
  // Knowledge graph
  // ---------------------------------------------------------------------

  external fun queryKnowledge(
    engineId: Long,
    subject: String?,
    predicate: String?,
    `object`: String?,
  ): String

  external fun entityContext(engineId: Long, entity: String): String

  // ---------------------------------------------------------------------
  // Sessions
  // ---------------------------------------------------------------------

  external fun startSession(engineId: Long, id: String)

  external fun endSession(engineId: Long, id: String): Boolean

  /** Returns `null` (not a thrown exception) if no session with this ID exists. */
  external fun getSession(engineId: Long, id: String): String?

  external fun listSessions(engineId: Long, limit: Int): String

  external fun consolidate(engineId: Long, sessionId: String, model: String?): String
}
