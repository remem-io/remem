use rememhq_core::ffi::*;
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[test]
fn test_ffi_engine_lifecycle_and_operations() {
    // Force mock provider to avoid external API calls
    std::env::set_var("REMEM_PROVIDER", "mock");

    // Use a temporary folder for configuration and SQLite database
    let temp_dir = tempfile::tempdir().unwrap();
    let data_dir_str = temp_dir.path().to_string_lossy().to_string();

    let project = CString::new("ffi-test-project").unwrap();
    let data_dir = CString::new(data_dir_str).unwrap();

    unsafe {
        let mut out_error: *mut c_char = std::ptr::null_mut();

        // 1. Initialize Engine
        let engine = remem_engine_new(
            project.as_ptr(),
            data_dir.as_ptr(),
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Engine new failed");
        assert!(!engine.is_null(), "Engine pointer was null");

        // 2. Store Memory
        let content = CString::new("FFI memory storage is highly efficient").unwrap();
        let tags_json = CString::new("[\"ffi\", \"test\"]").unwrap();
        let importance = 8.5_f32;

        let store_result = remem_store(
            engine,
            content.as_ptr(),
            tags_json.as_ptr(),
            importance,
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Store failed");
        assert!(!store_result.is_null(), "Store result was null");

        let store_json_str = CStr::from_ptr(store_result).to_string_lossy().to_string();
        let store_val: Value = serde_json::from_str(&store_json_str).unwrap();

        assert_eq!(
            store_val["content"],
            "FFI memory storage is highly efficient"
        );
        assert_eq!(store_val["tags"][0], "ffi");
        assert_eq!(store_val["tags"][1], "test");
        assert!((store_val["importance"].as_f64().unwrap() - 8.5).abs() < 0.01);

        let id_str = store_val["id"].as_str().unwrap().to_string();

        // Free store result
        remem_free_string(store_result);

        // 3. Search Memory (Vector + FTS)
        let query = CString::new("efficient memory").unwrap();
        let limit = 5;
        let filter_tags_json = CString::new("[\"ffi\"]").unwrap();

        let search_result = remem_search(
            engine,
            query.as_ptr(),
            limit,
            filter_tags_json.as_ptr(),
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Search failed");
        assert!(!search_result.is_null());

        let search_json_str = CStr::from_ptr(search_result).to_string_lossy().to_string();
        let search_val: Value = serde_json::from_str(&search_json_str).unwrap();
        assert!(search_val.is_array());
        assert!(!search_val.as_array().unwrap().is_empty());
        assert_eq!(
            search_val[0]["content"],
            "FFI memory storage is highly efficient"
        );

        remem_free_string(search_result);

        // 4. Recall Memory (LLM Guided)
        let recall_result = remem_recall(
            engine,
            query.as_ptr(),
            limit,
            std::ptr::null(), // No tag filter
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Recall failed");
        assert!(!recall_result.is_null());

        let recall_json_str = CStr::from_ptr(recall_result).to_string_lossy().to_string();
        let recall_val: Value = serde_json::from_str(&recall_json_str).unwrap();
        assert!(recall_val.is_array());
        assert!(!recall_val.as_array().unwrap().is_empty());

        remem_free_string(recall_result);

        // 5. Update Memory
        let id_cstr = CString::new(id_str.clone()).unwrap();
        let new_content = CString::new("FFI memory storage is incredibly performant").unwrap();
        let new_tags_json = CString::new("[\"ffi\", \"test\", \"updated\"]").unwrap();
        let new_importance = 9.2_f32;

        let update_result = remem_update(
            engine,
            id_cstr.as_ptr(),
            new_content.as_ptr(),
            new_importance,
            new_tags_json.as_ptr(),
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Update failed");
        assert!(!update_result.is_null());

        let update_json_str = CStr::from_ptr(update_result).to_string_lossy().to_string();
        let update_val: Value = serde_json::from_str(&update_json_str).unwrap();
        assert_eq!(
            update_val["content"],
            "FFI memory storage is incredibly performant"
        );
        assert_eq!(update_val["tags"].as_array().unwrap().len(), 3);
        assert!((update_val["importance"].as_f64().unwrap() - 9.2).abs() < 0.01);

        remem_free_string(update_result);

        // 6. Decay Memory
        let decay_count = remem_decay(engine, 0.5, &mut out_error as *mut *mut c_char);
        assert!(out_error.is_null(), "Decay failed");
        assert!(decay_count >= 0);

        // 7. Forget Memory
        let mode_cstr = CString::new("delete").unwrap();
        let forget_success = remem_forget(
            engine,
            id_cstr.as_ptr(),
            mode_cstr.as_ptr(),
            &mut out_error as *mut *mut c_char,
        );

        assert!(out_error.is_null(), "Forget failed");
        assert!(forget_success, "Forget should succeed");

        // 8. Clean up engine
        remem_engine_free(engine);
    }
}
