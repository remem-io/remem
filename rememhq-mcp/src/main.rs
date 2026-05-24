//! remem MCP server — exposes memory tools over stdio (JSON-RPC).
//!
//! Implements the Model Context Protocol for integration with
//! Claude Code, Cursor, and other MCP-compatible agents.

mod tools;
#[allow(dead_code)]
mod transport;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use rememhq_core::config::RememConfig;
use rememhq_core::providers::anthropic::AnthropicProvider;
use rememhq_core::providers::embeddings::OpenAIEmbeddings;
use rememhq_core::providers::google::{GoogleEmbeddings, GoogleProvider};
use rememhq_core::providers::openai::OpenAIProvider;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};

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
    let index = Arc::new(HNSWVectorIndex::new(768, 10000));

    // Load existing index if available
    let _ = index.load(&config.index_path()).await;

    // Create provider based on config
    let provider: Arc<dyn rememhq_core::providers::Provider> =
        match config.reasoning.provider.as_str() {
            "openai" => Arc::new(OpenAIProvider::new(None)?),
            "google" => Arc::new(GoogleProvider::new(None)?),
            "mock" => Arc::new(rememhq_core::providers::mock::MockProvider),
            _ => {
                // Default to anthropic; if key not set, try openai
                match AnthropicProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(OpenAIProvider::new(None)?),
                }
            }
        };

    // Embedding provider (Google, OpenAI, or Local)
    let embeddings: Arc<dyn rememhq_core::providers::EmbeddingProvider> =
        match config.reasoning.provider.as_str() {
            "google" => Arc::new(GoogleEmbeddings::new(None)?),
            "mock" => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
            "local" => {
                let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
                    .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
                let vocab_path = std::env::var("REMEM_LOCAL_VOCAB_PATH")
                    .unwrap_or_else(|_| "models/vocab.txt".to_string());
                Arc::new(rememhq_core::providers::local::LocalEmbeddings::new(
                    &model_path,
                    &vocab_path,
                )?)
            }
            _ => Arc::new(OpenAIEmbeddings::new(None, Some(768))?),
        };

    let engine = Arc::new(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index.clone(),
    ));

    tracing::info!(project = %args.project, "remem MCP server starting (stdio)");

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
                    "version": "0.1.0"
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
