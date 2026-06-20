// rememhq.h — C ABI for rememhq-core's high-level reasoning engine.
//
// ⚠️ VENDORED COPY. The source of truth is
// `rememhq-core/include/rememhq.h` in the main remem repository. This copy
// exists so the Swift package can build standalone (e.g. via Swift Package
// Index) without checking out the whole monorepo. Run
// `bindings/swift/scripts/sync-header.sh` from the repo root after editing
// the canonical header to keep this copy in sync.
// This header mirrors `rememhq-core/src/ffi/mod.rs` exactly. It is
// hand-maintained (not cbindgen-generated) — if you add, remove, or change
// the signature of an `extern "C"` function in ffi/mod.rs, update this file
// in the same change.
//
// Ownership conventions used throughout this API:
//   - Every non-null `char*` returned by a `remem_*` function that is NOT
//     an input parameter was allocated by Rust and MUST be freed by calling
//     `remem_free_string()` exactly once.
//   - `out_error`, when non-null, is set to a newly allocated error string
//     on failure (free with `remem_free_string()`), and left untouched on
//     success. Callers that don't care about error detail may pass NULL.
//   - `remem_engine_t*` is an opaque handle. Free it exactly once with
//     `remem_engine_free()` when done. Using it after freeing is undefined
//     behavior.
//   - JSON-returning functions (store/recall/search/etc.) return a
//     NUL-terminated UTF-8 JSON string. Parse it on the caller's side;
//     this header intentionally does not define mirrored C structs for
//     these payloads, since their shape can grow over time without
//     breaking ABI (new JSON fields are additive).
//
// Thread-safety: a `remem_engine_t*` may be shared across threads. Calls
// are dispatched onto an internal Tokio runtime and synchronize safely;
// no external locking is required.

#ifndef REMEMHQ_H
#define REMEMHQ_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#define REMEMHQ_API __declspec(dllexport)
#else
#define REMEMHQ_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

/// Opaque handle to a running reasoning engine (project config, storage,
/// vector index, and the configured reasoning/embedding providers).
typedef struct remem_engine_t remem_engine_t;

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Create a new engine for `project`, loading `.remem/config.toml` from
/// `data_dir` if present (pass NULL to use the default config search path).
///
/// Returns NULL on failure and, if `out_error` is non-null, sets it to a
/// freeable error string describing what went wrong.
REMEMHQ_API remem_engine_t* remem_engine_new(const char* project,
                                              const char* data_dir,
                                              char** out_error);

/// Free an engine handle. Safe to call with NULL.
REMEMHQ_API void remem_engine_free(remem_engine_t* engine);

/// Free a string previously returned by any `remem_*` function in this
/// header. Safe to call with NULL.
REMEMHQ_API void remem_free_string(char* ptr);

// ---------------------------------------------------------------------------
// Memory operations
// ---------------------------------------------------------------------------

/// Store a new memory. `tags_json` is a JSON array of strings, e.g.
/// `["preferences","ui"]`, or NULL for no tags. Pass a negative
/// `importance` to let the engine score importance automatically via the
/// configured reasoning model; pass a value in [0, 10] to set it explicitly.
///
/// Returns a JSON-encoded `MemoryRecord` on success, or NULL on failure.
REMEMHQ_API char* remem_store(remem_engine_t* engine,
                               const char* content,
                               const char* tags_json,
                               float importance,
                               char** out_error);

/// Guided recall: vector + keyword search, re-ranked by the reasoning
/// model for relevance. `filter_tags_json` is a JSON array of strings, or
/// NULL for no filter.
///
/// Returns a JSON-encoded array of `MemoryResult` on success, or NULL on
/// failure.
REMEMHQ_API char* remem_recall(remem_engine_t* engine,
                                const char* query,
                                size_t limit,
                                const char* filter_tags_json,
                                char** out_error);

/// Hybrid vector + keyword search without LLM re-ranking (faster, no
/// reasoning-model call). Same filter/return conventions as `remem_recall`.
REMEMHQ_API char* remem_search(remem_engine_t* engine,
                                const char* query,
                                size_t limit,
                                const char* filter_tags_json,
                                char** out_error);

/// Update an existing memory by ID (UUID string). Any of `content`,
/// `tags_json` may be NULL to leave that field unchanged; pass a negative
/// `importance` to leave importance unchanged.
///
/// Returns the JSON-encoded updated `MemoryRecord` on success, or NULL on
/// failure (including if no memory exists with that ID).
REMEMHQ_API char* remem_update(remem_engine_t* engine,
                                const char* id,
                                const char* content,
                                float importance,
                                const char* tags_json,
                                char** out_error);

/// Forget (remove) a memory by ID. `mode` is one of "delete", "decay", or
/// "archive" (case-insensitive); NULL defaults to "delete".
///
/// Returns true if a memory was found and the operation applied, false
/// otherwise (including on error — check `out_error` to distinguish "not
/// found" from a real failure).
REMEMHQ_API bool remem_forget(remem_engine_t* engine,
                               const char* id,
                               const char* mode,
                               char** out_error);

/// Apply importance-weighted decay across all active memories.
///
/// Returns the number of memories archived as a result, or -1 on failure.
REMEMHQ_API int32_t remem_decay(remem_engine_t* engine,
                                 float factor,
                                 char** out_error);

// ---------------------------------------------------------------------------
// Knowledge graph
// ---------------------------------------------------------------------------

/// Query the knowledge graph by subject/predicate/object triple, where any
/// of the three may be NULL as a wildcard.
///
/// Returns a JSON-encoded array of matching triples, or NULL on failure.
REMEMHQ_API char* remem_query_knowledge(remem_engine_t* engine,
                                        const char* subject,
                                        const char* predicate,
                                        const char* object,
                                        char** out_error);

/// Fetch every knowledge-graph triple touching `entity`, in either subject
/// or object position.
///
/// Returns a JSON-encoded array of triples, or NULL on failure.
REMEMHQ_API char* remem_get_entity_context(remem_engine_t* engine,
                                           const char* entity,
                                           char** out_error);

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// Create (start) a new tracked session with the given caller-supplied ID.
///
/// Returns true on success, false on failure.
REMEMHQ_API bool remem_session_create(remem_engine_t* engine,
                                      const char* session_id,
                                      char** out_error);

/// End a tracked session, stamping its end time.
///
/// Returns true if a session with that ID was found and ended, false if
/// no such session exists or the call failed (check `out_error`).
REMEMHQ_API bool remem_session_end(remem_engine_t* engine,
                                   const char* session_id,
                                   char** out_error);

/// Fetch a single session by ID.
///
/// Returns a JSON-encoded session record, or NULL if no session with that
/// ID exists (this is not itself an error — `out_error` is left untouched
/// in that case) or if the call failed (check `out_error`).
REMEMHQ_API char* remem_session_get(remem_engine_t* engine,
                                    const char* session_id,
                                    char** out_error);

/// List the `limit` most recent sessions for the engine's project.
///
/// Returns a JSON-encoded array of session records, or NULL on failure.
REMEMHQ_API char* remem_session_list(remem_engine_t* engine,
                                     size_t limit,
                                     char** out_error);

/// Run consolidation over a session's accumulated working memory,
/// extracting durable facts, detecting contradictions, and updating the
/// knowledge graph. Pass NULL for `model` to use the engine's configured
/// default reasoning model.
///
/// Returns a JSON-encoded `ConsolidationReport` on success, or NULL on
/// failure.
REMEMHQ_API char* remem_consolidate(remem_engine_t* engine,
                                    const char* session_id,
                                    const char* model,
                                    char** out_error);

#ifdef __cplusplus
}
#endif

#endif // REMEMHQ_H
