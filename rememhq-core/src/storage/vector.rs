use async_trait::async_trait;
use std::ffi::{CStr, CString};
use std::path::Path;
use uuid::Uuid;

pub mod remem_ffi {
    use std::os::raw::{c_char, c_float};

    #[repr(C)]
    pub struct RememSearchResult {
        pub id: [c_char; 40],
        pub similarity: c_float,
    }

    extern "C" {
        pub fn remem_index_new(dim: usize, max_elements: usize) -> *mut std::ffi::c_void;
        pub fn remem_index_free(index: *mut std::ffi::c_void);
        pub fn remem_index_add(
            index: *mut std::ffi::c_void,
            id: *const c_char,
            data: *const c_float,
            len: usize,
        );
        pub fn remem_index_remove(index: *mut std::ffi::c_void, id: *const c_char);
        pub fn remem_index_size(index: *mut std::ffi::c_void) -> usize;
        pub fn remem_index_search(
            index: *mut std::ffi::c_void,
            query: *const c_float,
            k: usize,
            out_count: *mut usize,
        ) -> *mut RememSearchResult;
        pub fn remem_free_results(results: *mut RememSearchResult);
        pub fn remem_index_save(index: *mut std::ffi::c_void, path: *const c_char);
        pub fn remem_index_load(index: *mut std::ffi::c_void, path: *const c_char);

        // Embedding Engine
        pub fn remem_embedder_new(
            model_path: *const c_char,
            vocab_path: *const c_char,
        ) -> *mut remem_embedder_t;
        pub fn remem_embedder_free(embedder: *mut remem_embedder_t);
        pub fn remem_embed_text(
            embedder: *mut remem_embedder_t,
            text: *const c_char,
            out_dim: *mut usize,
        ) -> *mut f32;
        pub fn remem_free_embedding(ptr: *mut f32);
        pub fn remem_embedder_dim(embedder: *mut remem_embedder_t) -> usize;
    }

    #[allow(non_camel_case_types)]
    pub enum remem_embedder_t {}
}

#[async_trait]
pub trait VectorIndex: Send + Sync {
    async fn add(&self, id: Uuid, embedding: &[f32]) -> anyhow::Result<()>;
    async fn remove(&self, id: Uuid) -> anyhow::Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<VectorResult>>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    async fn save(&self, path: &Path) -> anyhow::Result<()>;
    async fn load(&self, path: &Path) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct VectorResult {
    pub id: Uuid,
    pub similarity: f32,
}

pub struct HNSWVectorIndex {
    handle: *mut std::ffi::c_void,
}

impl HNSWVectorIndex {
    pub fn new(dim: usize, max_elements: usize) -> Self {
        unsafe {
            let handle = remem_ffi::remem_index_new(dim, max_elements);
            Self { handle }
        }
    }
}

impl Drop for HNSWVectorIndex {
    fn drop(&mut self) {
        unsafe {
            remem_ffi::remem_index_free(self.handle);
        }
    }
}

unsafe impl Send for HNSWVectorIndex {}
unsafe impl Sync for HNSWVectorIndex {}

#[async_trait]
impl VectorIndex for HNSWVectorIndex {
    async fn add(&self, id: Uuid, embedding: &[f32]) -> anyhow::Result<()> {
        let id_str = CString::new(id.to_string())?;
        unsafe {
            remem_ffi::remem_index_add(
                self.handle,
                id_str.as_ptr(),
                embedding.as_ptr(),
                embedding.len(),
            );
        }
        Ok(())
    }

    async fn remove(&self, id: Uuid) -> anyhow::Result<()> {
        let id_str = CString::new(id.to_string())?;
        unsafe {
            remem_ffi::remem_index_remove(self.handle, id_str.as_ptr());
        }
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<VectorResult>> {
        let mut count: usize = 0;
        unsafe {
            let results_ptr =
                remem_ffi::remem_index_search(self.handle, query.as_ptr(), k, &mut count);
            if results_ptr.is_null() {
                return Ok(vec![]);
            }

            let results_slice = std::slice::from_raw_parts(results_ptr, count);
            let mut output = Vec::with_capacity(count);

            for res in results_slice {
                let id_cstr = CStr::from_ptr(res.id.as_ptr());
                let id_str = id_cstr.to_string_lossy();
                if let Ok(uuid) = Uuid::parse_str(&id_str) {
                    output.push(VectorResult {
                        id: uuid,
                        similarity: res.similarity,
                    });
                }
            }

            remem_ffi::remem_free_results(results_ptr);
            Ok(output)
        }
    }

    fn len(&self) -> usize {
        unsafe { remem_ffi::remem_index_size(self.handle) }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    async fn save(&self, path: &Path) -> anyhow::Result<()> {
        let path_str = CString::new(path.to_string_lossy().to_string())?;
        unsafe {
            remem_ffi::remem_index_save(self.handle, path_str.as_ptr());
        }
        Ok(())
    }

    async fn load(&self, path: &Path) -> anyhow::Result<()> {
        let path_str = CString::new(path.to_string_lossy().to_string())?;
        unsafe {
            remem_ffi::remem_index_load(self.handle, path_str.as_ptr());
        }
        Ok(())
    }
}
