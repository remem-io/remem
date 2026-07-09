//! remem MCP server — exposes memory tools over stdio (JSON-RPC).
//!
//! Implements the Model Context Protocol for integration with
//! Claude Code, Codex, Cursor, Copilot, Antigravity CLI, OpenCode,
//! and any other MCP-compatible agent.

mod tools;
#[allow(dead_code)]
mod transport;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use rememhq_core::config::RememConfig;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
use rememhq_core::MemoryStore;

#[derive(Parser)]
#[command(name = "rememhq-mcp")]
struct Args {
    /// Project name for memory isolation.
    #[arg(long, default_value = "default")]
    project: String,
}

// --- JSON-RPC types ---

#[derive(Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("rememhq=info")
        .init();

    let args = Args::parse();
    let config = RememConfig::load(&args.project, None)?;

    // Initialize components
    let store = Arc::new(SqliteStore::open(&config.db_path())?);

    // Create providers using the centralised factory
    let provider = rememhq_core::providers::factory::build_reasoning_provider(&config);
    let embeddings = rememhq_core::providers::factory::build_embedding_provider(&config);

    let index = Arc::new(HNSWVectorIndex::new(embeddings.dimension(), 10000));
    let _ = index.load(&config.index_path()).await;

    let engine = Arc::new(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index.clone(),
        vec![],
    ));

    tracing::info!(project = %args.project, "remem MCP server starting (stdio)");

    if let Some(ref watch_dir) = config.memory.transcript_watch_dir {
        let watcher = rememhq_core::session::watcher::TranscriptWatcher::new(watch_dir);
        let mut rx = watcher.watch();
        let engine_clone = engine.clone();

        tokio::spawn(async move {
            while let Some(path) = rx.recv().await {
                // Determine session ID from filename (e.g. session_1234.jsonl)
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let session_id = stem.to_string();

                    tracing::info!("Extracting observations from {}", path.display());
                    match rememhq_core::session::extractors::TranscriptExtractor::extract_from_file(
                        &path,
                        &session_id,
                    ) {
                        Ok(observations) => {
                            let mut count = 0;
                            for obs in observations {
                                if let Err(e) =
                                    engine_clone.store.log_session_observation(&obs).await
                                {
                                    tracing::warn!("Failed to log observation: {}", e);
                                } else {
                                    count += 1;
                                }
                            }
                            tracing::info!(
                                "Imported {} observations for session {}",
                                count,
                                session_id
                            );

                            // Trigger consolidation
                            if let Err(e) = engine_clone
                                .compress_session_transcript(&session_id, None)
                                .await
                            {
                                tracing::error!("Failed to compress session transcript: {}", e);
                            }
                        }
                        Err(e) => tracing::error!(
                            "Failed to extract from transcript {}: {}",
                            path.display(),
                            e
                        ),
                    }
                }
            }
        });
    }

    // Run the stdio JSON-RPC loop
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    let shutdown_signal = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Shutdown signal received, exiting gracefully...");
    };

    tokio::select! {
        res = async {
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => handle_request(&engine, request).await,
                    Err(e) => Some(JsonRpcResponse::error(
                        serde_json::Value::Null,
                        -32700,
                        format!("Parse error: {}", e),
                    )),
                };

                if let Some(response) = response {
                    let json = serde_json::to_string(&response)?;
                    stdout.write_all(json.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
            Ok::<(), anyhow::Error>(())
        } => {
            if let Err(e) = res {
                tracing::error!("Error in stdin loop: {:?}", e);
            }
        }
        _ = shutdown_signal => {}
    }

    // Save index on exit
    tracing::info!("Saving vector index to {}", config.index_path().display());
    index.save(&config.index_path()).await?;

    Ok(())
}

async fn handle_request(
    engine: &Arc<ReasoningEngine>,
    request: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    let id = request.id.unwrap_or(serde_json::Value::Null);

    match request.method.as_str() {
        // MCP protocol methods
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "rememhq-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            Some(JsonRpcResponse::success(id, result))
        }

        // Notifications are fire-and-forget — no response per JSON-RPC spec
        method if method.starts_with("notifications/") => {
            tracing::debug!("Received notification: {}", method);
            None
        }

        "tools/list" => {
            let tools = tools::list_tools();
            Some(JsonRpcResponse::success(
                id,
                serde_json::json!({ "tools": tools }),
            ))
        }

        "tools/call" => match tools::call_tool(engine, &request.params).await {
            Ok(result) => Some(JsonRpcResponse::success(id, result)),
            Err(e) => Some(JsonRpcResponse::error(id, -32000, e.to_string())),
        },

        _ => Some(JsonRpcResponse::error(
            id,
            -32601,
            format!("Method not found: {}", request.method),
        )),
    }
}
