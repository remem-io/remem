//! C ABI exports for rememhq-core, consumed by Python cffi and Swift bindings.
//!
//! v0.1: Placeholder — the Python SDK uses the REST API instead of cffi.
//! v0.2+: Will expose store, recall, search, update, forget, consolidate
//! as C-callable functions for direct in-process binding.

// TODO (v0.2): C ABI wrappers around ReasoningEngine methods
// Example future API:
//
// #[no_mangle]
// pub extern "C" fn remem_store(
//     engine: *mut ReasoningEngine,
//     content: *const c_char,
//     tags_json: *const c_char,
//     importance: f32,
// ) -> *mut c_char { ... }
