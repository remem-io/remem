use crate::providers::EmbeddingProvider;
use crate::storage::vector::remem_ffi;
use async_trait::async_trait;

pub struct LocalEmbeddings {
    handle: *mut remem_ffi::remem_embedder_t,
    dim: usize,
}

// SAFETY: The C++ embedder is thread-safe if implemented correctly (stateless inference)
unsafe impl Send for LocalEmbeddings {}
unsafe impl Sync for LocalEmbeddings {}

impl LocalEmbeddings {
    pub fn new(model_path: &str, vocab_path: &str) -> anyhow::Result<Self> {
        let c_model_path = std::ffi::CString::new(model_path)?;
        let c_vocab_path = std::ffi::CString::new(vocab_path)?;
        let handle =
            unsafe { remem_ffi::remem_embedder_new(c_model_path.as_ptr(), c_vocab_path.as_ptr()) };

        if handle.is_null() {
            return Err(anyhow::anyhow!("Failed to initialize local embedder"));
        }

        let dim = unsafe { remem_ffi::remem_embedder_dim(handle) };

        Ok(Self { handle, dim })
    }
}

impl Drop for LocalEmbeddings {
    fn drop(&mut self) {
        unsafe {
            remem_ffi::remem_embedder_free(self.handle);
        }
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddings {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let c_text = std::ffi::CString::new(text)?;
        let mut out_dim = 0;

        let ptr =
            unsafe { remem_ffi::remem_embed_text(self.handle, c_text.as_ptr(), &mut out_dim) };

        if ptr.is_null() {
            return Err(anyhow::anyhow!("Local embedding failed"));
        }

        let vec = unsafe { std::slice::from_raw_parts(ptr, out_dim).to_vec() };

        unsafe {
            remem_ffi::remem_free_embedding(ptr);
        }

        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for t in texts {
            results.push(self.embed(t).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}
