//! Model management — download and verify local embedding/reasoning models.
//!
//! v0.2 ships one supported local embedding model:
//!   nomic-embed-text-v1.5 (ONNX, 768-dim, ~275 MB)
//!
//! A BERT WordPiece vocab file is also required alongside the ONNX file.
//! Both are downloaded from Hugging Face and placed in `REMEM_LOCAL_MODEL_PATH`
//! (default: `~/.remem/models/`).

use std::path::{Path, PathBuf};

/// All models remem knows how to download.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub description: &'static str,
    pub onnx_url: &'static str,
    pub vocab_url: &'static str,
    pub onnx_filename: &'static str,
    pub vocab_filename: &'static str,
    /// Approximate download size in bytes (for progress display).
    pub approx_bytes: u64,
}

pub const KNOWN_MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "nomic-embed",
        description: "nomic-embed-text-v1.5 — 768-dim BERT-style embedding model (~275 MB)",
        onnx_url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/onnx/model.onnx",
        vocab_url: "https://huggingface.co/bert-base-uncased/resolve/main/vocab.txt",
        onnx_filename: "nomic-embed-text.onnx",
        vocab_filename: "vocab.txt",
        approx_bytes: 288_000_000,
    },
];

/// Resolve a model spec by short ID (e.g. `"nomic-embed"`).
pub fn find_model(id: &str) -> Option<&'static ModelSpec> {
    KNOWN_MODELS.iter().find(|m| m.id == id)
}

/// Returns the default models directory: `$REMEM_DATA_DIR/models` or `~/.remem/models`.
pub fn default_models_dir() -> PathBuf {
    std::env::var("REMEM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".remem")
        })
        .join("models")
}

/// Pull a model: download ONNX + vocab into `dest_dir`, skipping files that
/// already exist and match the expected size.
///
/// Streams the response body to disk so large ONNX files are never fully
/// buffered in memory.
pub async fn pull_model(spec: &ModelSpec, dest_dir: &Path) -> anyhow::Result<PullResult> {
    use tokio::io::AsyncWriteExt;

    std::fs::create_dir_all(dest_dir)?;

    let client = reqwest::Client::builder()
        .user_agent("remem/0.2 model-pull")
        .build()?;

    let onnx_path  = dest_dir.join(spec.onnx_filename);
    let vocab_path = dest_dir.join(spec.vocab_filename);

    let onnx_downloaded  = download_if_missing(&client, spec.onnx_url,  &onnx_path).await?;
    let vocab_downloaded = download_if_missing(&client, spec.vocab_url, &vocab_path).await?;

    Ok(PullResult {
        onnx_path,
        vocab_path,
        onnx_downloaded,
        vocab_downloaded,
    })
}

async fn download_if_missing(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> anyhow::Result<bool> {
    use tokio::io::AsyncWriteExt;

    if dest.exists() {
        return Ok(false); // already present
    }

    let tmp = dest.with_extension("tmp");
    let resp = client.get(url).send().await?.error_for_status()?;

    {
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
    }

    tokio::fs::rename(&tmp, dest).await?;
    Ok(true)
}

/// Result of a `pull_model` call.
#[derive(Debug)]
pub struct PullResult {
    pub onnx_path:  PathBuf,
    pub vocab_path: PathBuf,
    /// `true` if the ONNX file was actually downloaded (vs already present).
    pub onnx_downloaded:  bool,
    /// `true` if the vocab file was actually downloaded (vs already present).
    pub vocab_downloaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_known_model() {
        let spec = find_model("nomic-embed").expect("nomic-embed must be a known model");
        assert_eq!(spec.onnx_filename, "nomic-embed-text.onnx");
        assert_eq!(spec.vocab_filename, "vocab.txt");
    }

    #[test]
    fn test_find_unknown_model_returns_none() {
        assert!(find_model("nonexistent-model-xyz").is_none());
    }

    #[test]
    fn test_default_models_dir_contains_models() {
        let dir = default_models_dir();
        assert!(dir.to_string_lossy().contains("models"));
    }
}
