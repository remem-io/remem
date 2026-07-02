use async_trait::async_trait;
use std::ffi::{CStr, CString};
use std::path::Path;
use uuid::Uuid;

pub mod remem_ffi {
    use std::os::raw::{c_char, c_float, c_int};

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
        ) -> c_int;
        pub fn remem_index_remove(index: *mut std::ffi::c_void, id: *const c_char) -> c_int;
        pub fn remem_index_size(index: *mut std::ffi::c_void) -> usize;
        pub fn remem_index_search(
            index: *mut std::ffi::c_void,
            query: *const c_float,
            k: usize,
            out_count: *mut usize,
        ) -> *mut RememSearchResult;
        pub fn remem_free_results(results: *mut RememSearchResult);
        pub fn remem_index_save(index: *mut std::ffi::c_void, path: *const c_char) -> c_int;
        pub fn remem_index_load(index: *mut std::ffi::c_void, path: *const c_char) -> c_int;

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

        // Document Chunker FFI
        pub fn remem_chunk_text(
            text: *const c_char,
            chunk_size: usize,
            chunk_overlap: usize,
            by_words: c_int,
        ) -> *mut std::ffi::c_void;
        pub fn remem_chunks_count(chunks: *mut std::ffi::c_void) -> usize;
        pub fn remem_chunks_get(chunks: *mut std::ffi::c_void, index: usize) -> *const c_char;
        pub fn remem_chunks_free(chunks: *mut std::ffi::c_void);
        pub fn remem_normalize_text(
            text: *const c_char,
            to_lower: c_int,
            strip_whitespace: c_int,
        ) -> *mut c_char;
        pub fn remem_free_string_cpp(str: *mut c_char);
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
            let res = remem_ffi::remem_index_add(
                self.handle,
                id_str.as_ptr(),
                embedding.as_ptr(),
                embedding.len(),
            );
            if res != 0 {
                anyhow::bail!("Failed to add embedding to vector index");
            }
        }
        Ok(())
    }

    async fn remove(&self, id: Uuid) -> anyhow::Result<()> {
        let id_str = CString::new(id.to_string())?;
        unsafe {
            let res = remem_ffi::remem_index_remove(self.handle, id_str.as_ptr());
            if res != 0 {
                anyhow::bail!("Failed to remove embedding from vector index");
            }
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
            let res = remem_ffi::remem_index_save(self.handle, path_str.as_ptr());
            if res != 0 {
                anyhow::bail!("Failed to save vector index");
            }
        }
        Ok(())
    }

    async fn load(&self, path: &Path) -> anyhow::Result<()> {
        let path_str = CString::new(path.to_string_lossy().to_string())?;
        unsafe {
            let res = remem_ffi::remem_index_load(self.handle, path_str.as_ptr());
            if res != 0 {
                anyhow::bail!("Failed to load vector index");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    #[test]
    fn test_cpp_text_normalization() {
        let input = CString::new("   Hello   World! \n This is   a   Test   ").unwrap();
        unsafe {
            let normalized_ptr = remem_ffi::remem_normalize_text(input.as_ptr(), 1, 1);
            assert!(!normalized_ptr.is_null());

            let normalized_str = CStr::from_ptr(normalized_ptr).to_string_lossy();
            assert_eq!(normalized_str, "hello world! this is a test");

            remem_ffi::remem_free_string_cpp(normalized_ptr);
        }
    }

    #[test]
    fn test_cpp_document_chunker_by_words() {
        let input = CString::new("one two three four five six").unwrap();
        unsafe {
            // chunk_size = 3, chunk_overlap = 1, by_words = 1
            let chunks_ptr = remem_ffi::remem_chunk_text(input.as_ptr(), 3, 1, 1);
            assert!(!chunks_ptr.is_null());

            let count = remem_ffi::remem_chunks_count(chunks_ptr);
            assert_eq!(count, 3); // ["one two three", "three four five", "five six"]

            let c0 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 0)).to_string_lossy();
            let c1 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 1)).to_string_lossy();
            let c2 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 2)).to_string_lossy();

            assert_eq!(c0, "one two three");
            assert_eq!(c1, "three four five");
            assert_eq!(c2, "five six");

            remem_ffi::remem_chunks_free(chunks_ptr);
        }
    }

    #[test]
    fn test_cpp_document_chunker_by_chars() {
        let input = CString::new("abcdefgh").unwrap();
        unsafe {
            // chunk_size = 4, chunk_overlap = 2, by_words = 0
            let chunks_ptr = remem_ffi::remem_chunk_text(input.as_ptr(), 4, 2, 0);
            assert!(!chunks_ptr.is_null());

            let count = remem_ffi::remem_chunks_count(chunks_ptr);
            assert_eq!(count, 3); // ["abcd", "cdef", "efgh"]

            let c0 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 0)).to_string_lossy();
            let c1 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 1)).to_string_lossy();
            let c2 = CStr::from_ptr(remem_ffi::remem_chunks_get(chunks_ptr, 2)).to_string_lossy();

            assert_eq!(c0, "abcd");
            assert_eq!(c1, "cdef");
            assert_eq!(c2, "efgh");

            remem_ffi::remem_chunks_free(chunks_ptr);
        }
    }
}
