import ctypes
import os
import json
import sys
import tempfile

def main():
    print("=== Remem C ABI / FFI Python ctypes Test ===")
    
    # 1. Determine the path of the compiled rememhq_core dynamic library
    # For Windows: target/debug/rememhq_core.dll
    # For macOS: target/debug/librememhq_core.dylib
    # For Linux: target/debug/librememhq_core.so
    
    workspace_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    
    dll_name = "rememhq_core.dll"
    if sys.platform == "darwin":
        dll_name = "librememhq_core.dylib"
    elif sys.platform.startswith("linux"):
        dll_name = "librememhq_core.so"
        
    release_path = os.path.join(workspace_root, "target", "release", dll_name)
    debug_path = os.path.join(workspace_root, "target", "debug", dll_name)
    
    if os.path.exists(release_path) and os.path.getsize(release_path) > 0:
        dll_path = release_path
    else:
        dll_path = debug_path
        
    print(f"Loading dynamic library from: {dll_path}")
    
    if not os.path.exists(dll_path) or os.path.getsize(dll_path) == 0:
        print(f"ERROR: Library not found or empty at {dll_path}.")
        print("Please ensure you have compiled the workspace (e.g. via cargo build or cargo build --release).")
        sys.exit(1)
        
    try:
        lib = ctypes.CDLL(dll_path)
    except Exception as e:
        print(f"ERROR: Failed to load dynamic library: {e}")
        sys.exit(1)
        
    # 2. Define C Function Signatures
    # remem_engine_new
    lib.remem_engine_new.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_char_p)]
    lib.remem_engine_new.restype = ctypes.c_void_p
    
    # remem_engine_free
    lib.remem_engine_free.argtypes = [ctypes.c_void_p]
    lib.remem_engine_free.restype = None
    
    # remem_store
    lib.remem_store.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_float, ctypes.POINTER(ctypes.c_char_p)]
    lib.remem_store.restype = ctypes.c_void_p
    
    # remem_search
    lib.remem_search.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.POINTER(ctypes.c_char_p)]
    lib.remem_search.restype = ctypes.c_void_p
    
    # remem_free_string
    lib.remem_free_string.argtypes = [ctypes.c_void_p]
    lib.remem_free_string.restype = None

    # Force mock provider for offline FFI test
    os.environ["REMEM_PROVIDER"] = "mock"
    
    # 3. Create temp data directory
    with tempfile.TemporaryDirectory() as temp_dir:
        print(f"Created temporary database folder: {temp_dir}")
        
        project_b = b"python-ffi-project"
        data_dir_b = temp_dir.encode('utf-8')
        
        error_ptr = ctypes.c_char_p()
        
        # Initialize Engine
        print("Initializing remem engine via C FFI...")
        engine = lib.remem_engine_new(project_b, data_dir_b, ctypes.byref(error_ptr))
        
        if error_ptr.value:
            print(f"ERROR: Engine initialization failed: {error_ptr.value.decode('utf-8')}")
            lib.remem_free_string(error_ptr)
            sys.exit(1)
            
        if not engine:
            print("ERROR: Engine pointer was null")
            sys.exit(1)
            
        print("OK: Engine initialized successfully!")
        
        # Store Memory
        content_b = b"Remem C FFI is working beautifully inside Python ctypes"
        tags_json_b = b'["python", "ffi", "ctypes"]'
        importance = ctypes.c_float(9.5)
        
        print("\nStoring a new memory record via FFI...")
        store_res_ptr = lib.remem_store(engine, content_b, tags_json_b, importance, ctypes.byref(error_ptr))
        
        if error_ptr.value:
            print(f"ERROR: Store failed: {error_ptr.value.decode('utf-8')}")
            lib.remem_free_string(error_ptr)
            lib.remem_engine_free(engine)
            sys.exit(1)
            
        store_res = ctypes.string_at(store_res_ptr).decode('utf-8')
        print(f"OK: Stored successfully! Returned JSON string:\n  {store_res}")
        
        store_data = json.loads(store_res)
        print(f"  Memory UUID: {store_data.get('id')}")
        print(f"  Importance: {store_data.get('importance')}")
        
        # Free Rust allocated string
        lib.remem_free_string(store_res_ptr)
        
        # Search Memory
        query_b = b"Python FFI"
        limit = ctypes.c_int(5)
        filter_tags_json_b = b'["python"]'
        
        print("\nSearching stored memories via FFI...")
        search_res_ptr = lib.remem_search(engine, query_b, limit, filter_tags_json_b, ctypes.byref(error_ptr))
        
        if error_ptr.value:
            print(f"ERROR: Search failed: {error_ptr.value.decode('utf-8')}")
            lib.remem_free_string(error_ptr)
            lib.remem_engine_free(engine)
            sys.exit(1)
            
        search_res = ctypes.string_at(search_res_ptr).decode('utf-8')
        print(f"OK: Search successfully! Returned JSON results:\n  {search_res}")
        
        # Free search results string
        lib.remem_free_string(search_res_ptr)
        
        # Free Engine
        print("\nFreeing remem engine...")
        lib.remem_engine_free(engine)
        print("OK: Engine freed successfully!")
        
    print("\n=== Remem C ABI / FFI Python ctypes Test Passed! ===")

if __name__ == "__main__":
    main()
