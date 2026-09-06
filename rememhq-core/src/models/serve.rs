//! Spawn and manage a local llama.cpp-compatible inference server
//! (`llama-server`) for a downloaded GGUF model.
//!
//! remem does not implement its own LLM inference runtime. The pragmatic
//! MVP path — and the one this module takes — is to run GGUF weights
//! through an existing, well-optimized llama.cpp server process and talk
//! to it over its OpenAI-compatible HTTP API. [`crate::providers::local::LocalProvider`]
//! already speaks that API (`LLAMA_API_BASE` / `OLLAMA_API_BASE`); this
//! module just makes starting the server a one-command experience instead
//! of a manual `llama-server -m ... --port ...` invocation the user has to
//! assemble by hand.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};

/// Default port `remem models serve` binds to when none is given.
pub const DEFAULT_PORT: u16 = 8080;

/// Default concurrent request slots (`--parallel`). remem's own
/// reasoning pipeline can have several provider calls in flight at once
/// (see `ProviderPool`'s semaphore, default 20) — llama-server's own
/// default of 1 slot would serialize every one of them regardless of
/// that, so this picks something greater than 1 without assuming the
/// abundant memory a cloud API doesn't need to worry about (each slot
/// multiplies KV-cache memory usage).
pub const DEFAULT_PARALLEL_SLOTS: u32 = 2;

/// `-ngl` value used when GPU acceleration is auto-detected: large
/// enough that llama.cpp offloads every layer that fits, rather than
/// remem trying to calculate an exact number itself. llama.cpp clamps
/// this to what actually fits and falls back to CPU for the rest, so
/// there's no failure mode from guessing too high — only from guessing
/// too low (leaving a capable GPU under-used) or defaulting to `0`
/// unconditionally (leaving it unused entirely, the previous behavior
/// this replaces).
const OFFLOAD_ALL_LAYERS: u32 = 999;

/// How long to wait for the server's `/health` endpoint before giving up.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A running local inference server process.
pub struct LocalServer {
    child: Child,
    pub port: u16,
    pub base_url: String,
    pub binary: String,
}

impl LocalServer {
    /// The OpenAI-compatible chat completions base URL — what
    /// `LLAMA_API_BASE` (or `OLLAMA_API_BASE`) should be set to.
    pub fn api_base(&self) -> String {
        format!("{}/v1", self.base_url)
    }

    /// Terminate the server process.
    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.child.kill().await.ok();
        Ok(())
    }

    /// Wait for the process to exit on its own (crash, killed externally,
    /// etc). Callers that want to block until Ctrl+C should race this
    /// against a signal future instead of awaiting it directly.
    pub async fn wait(&mut self) -> anyhow::Result<std::process::ExitStatus> {
        Ok(self.child.wait().await?)
    }
}

/// Locate the llama.cpp server binary: `$REMEM_LLAMA_SERVER_BIN` if set
/// (used as-is, no PATH search — lets the operator point at a specific
/// build), otherwise `llama-server` on `PATH`.
pub fn find_server_binary() -> Option<String> {
    if let Ok(bin) = std::env::var("REMEM_LLAMA_SERVER_BIN") {
        if !bin.trim().is_empty() {
            return Some(bin);
        }
    }
    which_on_path("llama-server")
}

fn which_on_path(name: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
        #[cfg(windows)]
        {
            let candidate_exe = dir.join(format!("{name}.exe"));
            if candidate_exe.is_file() {
                return Some(candidate_exe.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Best-effort guess at whether this machine likely has GPU acceleration
/// available to llama.cpp — used only to pick a friendlier *default* for
/// `-ngl` (see [`resolve_gpu_layers`]); an explicit `--gpu-layers`
/// always overrides it regardless. Getting this wrong in either
/// direction only costs performance, never correctness: an
/// over-optimistic `999` gets clamped down by llama.cpp to whatever
/// actually fits (with the excess quietly staying on CPU), and an
/// under-optimistic `0` just leaves a capable GPU idle.
fn likely_has_gpu_acceleration() -> bool {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        // Apple Silicon — llama.cpp's official builds enable the Metal
        // backend by default, and it's present on every such Mac (unlike
        // NVIDIA/CUDA below, where the GPU itself might not be there).
        return true;
    }
    // Linux/Windows: `nvidia-smi` on PATH is a reasonable signal an
    // NVIDIA GPU and its driver are present, without needing to link
    // against CUDA (or parse its output — its mere presence is enough).
    which_on_path("nvidia-smi").is_some()
}

/// Resolve how many layers to offload to GPU (`-ngl`): `explicit` if
/// given (including `Some(0)`, to force CPU-only on GPU-capable
/// hardware), otherwise a guess from [`likely_has_gpu_acceleration`].
pub fn resolve_gpu_layers(explicit: Option<u32>) -> u32 {
    explicit.unwrap_or_else(|| {
        if likely_has_gpu_acceleration() {
            OFFLOAD_ALL_LAYERS
        } else {
            0
        }
    })
}

/// Options for [`spawn`].
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub port: u16,
    pub host: String,
    /// Context window size, passed as `-c`.
    pub ctx_size: u32,
    /// Layers offloaded to GPU, passed as `-ngl`. `None` auto-detects
    /// (see [`resolve_gpu_layers`]); `Some(0)` forces CPU-only.
    pub gpu_layers: Option<u32>,
    /// Concurrent request slots, passed as `--parallel`.
    pub parallel_slots: u32,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host: "127.0.0.1".to_string(),
            ctx_size: 4096,
            gpu_layers: None,
            parallel_slots: DEFAULT_PARALLEL_SLOTS,
        }
    }
}

/// Spawn `llama-server` (or `$REMEM_LLAMA_SERVER_BIN`) against `model_path`
/// and wait until its `/health` endpoint reports ready.
///
/// Fails fast (rather than waiting out the full timeout) if the binary
/// can't be found, the model file doesn't exist, or the child process
/// exits before becoming healthy.
pub async fn spawn(model_path: &Path, opts: &ServeOptions) -> anyhow::Result<LocalServer> {
    if !model_path.exists() {
        anyhow::bail!(
            "model file not found at {}. Run `remem models pull <id>` first.",
            model_path.display()
        );
    }

    let binary = find_server_binary().ok_or_else(|| {
        anyhow::anyhow!(
            "no llama.cpp server binary found. Install `llama-server` (from llama.cpp) and \
             make sure it's on PATH, or set REMEM_LLAMA_SERVER_BIN to its full path. \
             See: https://github.com/ggml-org/llama.cpp"
        )
    })?;

    let resolved_gpu_layers = resolve_gpu_layers(opts.gpu_layers);

    let child = Command::new(&binary)
        .arg("-m")
        .arg(model_path)
        .arg("--port")
        .arg(opts.port.to_string())
        .arg("--host")
        .arg(&opts.host)
        .arg("-c")
        .arg(opts.ctx_size.to_string())
        .arg("-ngl")
        .arg(resolved_gpu_layers.to_string())
        .arg("--parallel")
        .arg(opts.parallel_slots.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start {}: {}", binary, e))?;

    let base_url = format!("http://{}:{}", opts.host, opts.port);
    let mut server = LocalServer {
        child,
        port: opts.port,
        base_url,
        binary,
    };

    wait_until_ready(&mut server).await?;
    Ok(server)
}

async fn wait_until_ready(server: &mut LocalServer) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let health_url = format!("{}/health", server.base_url);
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;

    loop {
        // The process may have already exited (bad model, port in use,
        // missing shared libs, ...) — surface that instead of spinning
        // silently until the timeout.
        if let Some(status) = server.child.try_wait()? {
            anyhow::bail!(
                "{} exited before becoming ready (status: {})",
                server.binary,
                status
            );
        }

        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s waiting for {} to become ready on {}",
                READY_TIMEOUT.as_secs(),
                server.binary,
                server.base_url
            );
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_serve_options() {
        let opts = ServeOptions::default();
        assert_eq!(opts.port, DEFAULT_PORT);
        assert_eq!(opts.host, "127.0.0.1");
        assert_eq!(opts.gpu_layers, None, "auto-detect by default");
        assert_eq!(opts.parallel_slots, DEFAULT_PARALLEL_SLOTS);
    }

    #[test]
    fn test_resolve_gpu_layers_explicit_zero_forces_cpu() {
        // Some(0) must win even on hardware likely_has_gpu_acceleration()
        // would say yes to — explicit --gpu-layers 0 is how a person
        // forces CPU-only, and auto-detection must never override it.
        assert_eq!(resolve_gpu_layers(Some(0)), 0);
    }

    #[test]
    fn test_resolve_gpu_layers_explicit_value_always_wins() {
        assert_eq!(resolve_gpu_layers(Some(17)), 17);
        assert_eq!(resolve_gpu_layers(Some(999)), 999);
    }

    #[test]
    fn test_resolve_gpu_layers_auto_detect_is_deterministic() {
        // Can't assert a specific true/false outcome without knowing the
        // test runner's hardware, but the same machine must always
        // resolve the same way within one run.
        let a = resolve_gpu_layers(None);
        let b = resolve_gpu_layers(None);
        assert_eq!(a, b);
        assert!(a == 0 || a == OFFLOAD_ALL_LAYERS);
    }

    #[test]
    fn test_find_server_binary_honors_env_override() {
        // Serialized via an env-mutating test lock pattern would be ideal,
        // but this crate doesn't expose one publicly; scope the mutation
        // tightly and restore it immediately either way.
        let prev = std::env::var("REMEM_LLAMA_SERVER_BIN").ok();
        std::env::set_var("REMEM_LLAMA_SERVER_BIN", "/opt/custom/llama-server");
        let found = find_server_binary();
        match prev {
            Some(v) => std::env::set_var("REMEM_LLAMA_SERVER_BIN", v),
            None => std::env::remove_var("REMEM_LLAMA_SERVER_BIN"),
        }
        assert_eq!(found.as_deref(), Some("/opt/custom/llama-server"));
    }

    #[test]
    fn test_which_on_path_finds_a_real_binary() {
        // `sh` exists in essentially every PATH in CI/dev containers —
        // use it as a stand-in to exercise the PATH-search branch itself
        // without depending on llama-server being installed.
        let found = which_on_path("sh");
        assert!(found.is_some(), "expected to find `sh` on PATH");
    }

    #[test]
    fn test_which_on_path_missing_binary_returns_none() {
        assert!(which_on_path("definitely-not-a-real-binary-xyz").is_none());
    }

    #[tokio::test]
    async fn test_spawn_fails_fast_on_missing_model_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.gguf");
        let err = spawn(&missing, &ServeOptions::default())
            .await
            .expect_err("should fail before ever touching a binary");
        assert!(err.to_string().contains("not found"));
    }
}
