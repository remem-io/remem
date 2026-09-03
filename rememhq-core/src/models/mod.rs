//! Model management — download and verify local embedding/reasoning models.
//!
//! remem ships two kinds of downloadable local model:
//!
//! - [`ModelKind::Embedding`] — an ONNX embedding model paired with a
//!   tokenizer/vocab file, consumed by [`crate::providers::local::LocalEmbeddings`]
//!   via the `libremem` C++ FFI bridge. Currently: `nomic-embed`.
//! - [`ModelKind::LocalLlm`] — a single-file GGUF model intended for a
//!   llama.cpp-compatible server (`llama-server`, Ollama, LM Studio, ...).
//!   remem itself only downloads the weights; running inference against
//!   them is handled by [`crate::providers::local::LocalProvider`] talking
//!   to `LLAMA_API_BASE` / `OLLAMA_API_BASE`. Currently: `phi-3-mini`.
//!
//! All models are downloaded from Hugging Face and placed under
//! `default_models_dir()` (default: `~/.remem/models/`).

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

/// What a [`ModelSpec`] is used for, and therefore what artifacts it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// ONNX embedding model + a paired tokenizer/vocab file.
    Embedding,
    /// Single-file GGUF weights for a local (llama.cpp-compatible) LLM.
    LocalLlm,
}

/// Metadata for a single downloadable model.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub description: &'static str,
    pub kind: ModelKind,
    /// URL of the model's main weights file (ONNX or GGUF).
    pub primary_url: &'static str,
    /// Filename the main weights file is saved under in the models dir.
    pub primary_filename: &'static str,
    /// URL of a secondary artifact (currently: the tokenizer/vocab file
    /// required by [`ModelKind::Embedding`] models). `None` for models that
    /// are self-contained in a single file, such as GGUF local LLMs.
    pub secondary_url: Option<&'static str>,
    /// Filename the secondary artifact is saved under, if any.
    pub secondary_filename: Option<&'static str>,
    /// Approximate download size in bytes (for progress display).
    pub approx_bytes: u64,
}

pub const KNOWN_MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "nomic-embed",
        description: "nomic-embed-text-v1.5 — 768-dim BERT-style embedding model (~275 MB)",
        kind: ModelKind::Embedding,
        primary_url:
            "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/onnx/model.onnx",
        primary_filename: "nomic-embed-text.onnx",
        secondary_url: Some("https://huggingface.co/bert-base-uncased/resolve/main/vocab.txt"),
        secondary_filename: Some("vocab.txt"),
        approx_bytes: 288_000_000,
    },
    ModelSpec {
        id: "phi-3-mini",
        description:
            "Phi-3-mini-4k-instruct — 3.8B local reasoning model, Q4_K_M GGUF (~2.4 GB). \
             Serve it with `llama-server -m <path>` (or import into Ollama) and point \
             LLAMA_API_BASE / OLLAMA_API_BASE at it.",
        kind: ModelKind::LocalLlm,
        primary_url: "https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf",
        primary_filename: "phi-3-mini-4k-instruct-q4.gguf",
        secondary_url: None,
        secondary_filename: None,
        approx_bytes: 2_393_000_000,
    },
];

/// Resolve a model spec by short ID (e.g. `"nomic-embed"`, `"phi-3-mini"`).
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

/// Local installation state of a [`ModelSpec`], derived by checking for its
/// artifact(s) on disk (see [`install_status`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    /// No artifacts present.
    NotInstalled,
    /// Some but not all required artifacts are present (e.g. the vocab
    /// file was downloaded but the ONNX weights were not, or vice versa).
    PartiallyInstalled,
    /// Every artifact the model needs is present on disk.
    Installed,
}

impl InstallStatus {
    /// Short human-readable label, used by both the CLI and the REST API.
    pub fn label(self) -> &'static str {
        match self {
            InstallStatus::NotInstalled => "not installed",
            InstallStatus::PartiallyInstalled => "partially installed",
            InstallStatus::Installed => "installed",
        }
    }
}

/// Check `dest_dir` for a model's artifact(s) and report how much of it is present.
pub fn install_status(spec: &ModelSpec, dest_dir: &Path) -> InstallStatus {
    let primary_present = dest_dir.join(spec.primary_filename).exists();
    let secondary_present = spec
        .secondary_filename
        .map(|f| dest_dir.join(f).exists());

    match secondary_present {
        // Single-file model: no secondary artifact is expected at all.
        None => {
            if primary_present {
                InstallStatus::Installed
            } else {
                InstallStatus::NotInstalled
            }
        }
        Some(true) if primary_present => InstallStatus::Installed,
        Some(false) if !primary_present => InstallStatus::NotInstalled,
        Some(_) => InstallStatus::PartiallyInstalled,
    }
}

/// Pull a model: download its primary artifact (and secondary, if any) into
/// `dest_dir`, skipping files that already exist. Streams the response body
/// so large weight files are never fully buffered in memory.
pub async fn pull_model(spec: &ModelSpec, dest_dir: &Path) -> anyhow::Result<PullResult> {
    std::fs::create_dir_all(dest_dir)?;

    let client = reqwest::Client::builder()
        .user_agent("remem/0.2 model-pull")
        .build()?;

    let primary_path = dest_dir.join(spec.primary_filename);
    let primary_downloaded = download_if_missing(&client, spec.primary_url, &primary_path).await?;

    let (secondary_path, secondary_downloaded) = match (spec.secondary_url, spec.secondary_filename)
    {
        (Some(url), Some(filename)) => {
            let path = dest_dir.join(filename);
            let downloaded = download_if_missing(&client, url, &path).await?;
            (Some(path), downloaded)
        }
        _ => (None, false),
    };

    Ok(PullResult {
        primary_path,
        primary_downloaded,
        secondary_path,
        secondary_downloaded,
    })
}

async fn download_if_missing(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> anyhow::Result<bool> {
    if dest.exists() {
        return Ok(false);
    }

    let tmp = dest.with_extension("tmp");
    let resp = client.get(url).send().await?.error_for_status()?;

    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp, dest).await?;
    Ok(true)
}

/// Result of a [`pull_model`] call.
#[derive(Debug)]
pub struct PullResult {
    pub primary_path: PathBuf,
    /// `true` if the primary artifact was actually downloaded (vs already present).
    pub primary_downloaded: bool,
    pub secondary_path: Option<PathBuf>,
    /// `true` if the secondary artifact was actually downloaded (vs already present).
    pub secondary_downloaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_known_embedding_model() {
        let spec = find_model("nomic-embed").expect("nomic-embed must be a known model");
        assert_eq!(spec.kind, ModelKind::Embedding);
        assert_eq!(spec.primary_filename, "nomic-embed-text.onnx");
        assert_eq!(spec.secondary_filename, Some("vocab.txt"));
    }

    #[test]
    fn test_find_known_local_llm_model() {
        let spec = find_model("phi-3-mini").expect("phi-3-mini must be a known model");
        assert_eq!(spec.kind, ModelKind::LocalLlm);
        assert_eq!(spec.primary_filename, "phi-3-mini-4k-instruct-q4.gguf");
        assert_eq!(spec.secondary_filename, None);
        assert_eq!(spec.secondary_url, None);
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

    #[test]
    fn test_install_status_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let spec = find_model("nomic-embed").unwrap();
        assert_eq!(install_status(spec, dir.path()), InstallStatus::NotInstalled);
    }

    #[test]
    fn test_install_status_partially_installed_two_file_model() {
        let dir = tempfile::tempdir().unwrap();
        let spec = find_model("nomic-embed").unwrap();
        std::fs::write(dir.path().join(spec.primary_filename), b"fake-onnx").unwrap();
        assert_eq!(
            install_status(spec, dir.path()),
            InstallStatus::PartiallyInstalled
        );
    }

    #[test]
    fn test_install_status_installed_two_file_model() {
        let dir = tempfile::tempdir().unwrap();
        let spec = find_model("nomic-embed").unwrap();
        std::fs::write(dir.path().join(spec.primary_filename), b"fake-onnx").unwrap();
        std::fs::write(dir.path().join(spec.secondary_filename.unwrap()), b"fake-vocab").unwrap();
        assert_eq!(install_status(spec, dir.path()), InstallStatus::Installed);
    }

    #[test]
    fn test_install_status_single_file_model() {
        let dir = tempfile::tempdir().unwrap();
        let spec = find_model("phi-3-mini").unwrap();
        assert_eq!(install_status(spec, dir.path()), InstallStatus::NotInstalled);
        std::fs::write(dir.path().join(spec.primary_filename), b"fake-gguf").unwrap();
        assert_eq!(install_status(spec, dir.path()), InstallStatus::Installed);
    }

    #[test]
    fn test_all_known_models_have_unique_ids() {
        let mut ids: Vec<&str> = KNOWN_MODELS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN_MODELS.len(), "duplicate model id in KNOWN_MODELS");
    }
}
