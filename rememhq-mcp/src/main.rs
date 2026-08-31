//! remem MCP server — exposes memory tools over stdio (JSON-RPC).
//!
//! Implements the Model Context Protocol for integration with
//! Claude Code, Codex, Cursor, Copilot, Antigravity CLI, OpenCode,
//! and any other MCP-compatible agent.

mod tools;
mod transport;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
                let mut session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Special handling for antigravity-cli nested transcripts
                if session_id == "transcript" || session_id == "transcript_full" {
                    if let Some(logs_dir) = path.parent() {
                        if logs_dir.file_name().and_then(|n| n.to_str()) == Some("logs") {
                            if let Some(sys_gen) = logs_dir.parent() {
                                if sys_gen.file_name().and_then(|n| n.to_str())
                                    == Some(".system_generated")
                                {
                                    if let Some(conv_dir) = sys_gen.parent() {
                                        if let Some(conv_id) =
                                            conv_dir.file_name().and_then(|n| n.to_str())
                                        {
                                            session_id = conv_id.to_string();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if session_id != "unknown" {
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

    let session_id = format!("mcp-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let active_client_name = Arc::new(tokio::sync::RwLock::new(None::<String>));

    // Run the stdio JSON-RPC loop using the transport module abstraction
    let shutdown_signal = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Shutdown signal received, exiting gracefully...");
    };

    let session_id_clone = session_id.clone();
    let client_name_clone = active_client_name.clone();
    let engine_for_disconnect = engine.clone();
    tokio::select! {
        res = transport::stdio::run_stdio_loop(move |line| {
            let engine = engine.clone();
            let session_id = session_id_clone.clone();
            let client_name_state = client_name_clone.clone();
            async move {
                let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => handle_request(&engine, request, &session_id, client_name_state).await,
                    Err(e) => Some(JsonRpcResponse::error(
                        serde_json::Value::Null,
                        -32700,
                        format!("Parse error: {}", e),
                    )),
                };
                response.and_then(|resp| serde_json::to_string(&resp).ok())
            }
        }) => {
            if let Err(e) = res {
                tracing::error!("Error in stdio loop: {:?}", e);
            }
        }
        _ = shutdown_signal => {}
    }

    if let Some(agent_name) = active_client_name.read().await.clone() {
        engine_for_disconnect.emit_event(
            rememhq_core::reasoning::ReasoningEvent::AgentDisconnected {
                session_id: session_id.clone(),
                agent_name,
            },
        );
    }

    // Save index on exit
    tracing::info!("Saving vector index to {}", config.index_path().display());
    index.save(&config.index_path()).await?;

    Ok(())
}

async fn handle_request(
    engine: &Arc<ReasoningEngine>,
    request: JsonRpcRequest,
    session_id: &str,
    active_client_name: Arc<tokio::sync::RwLock<Option<String>>>,
) -> Option<JsonRpcResponse> {
    let id = request.id.unwrap_or(serde_json::Value::Null);

    match request.method.as_str() {
        // MCP protocol methods
        "initialize" => {
            let client_name = request
                .params
                .get("clientInfo")
                .and_then(|info| info.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown Agent")
                .to_string();

            let client_version = request
                .params
                .get("clientInfo")
                .and_then(|info| info.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .to_string();

            tracing::info!(
                agent_name = %client_name,
                agent_version = %client_version,
                "Client connected via MCP initialize"
            );

            *active_client_name.write().await = Some(client_name.clone());

            engine.emit_event(rememhq_core::reasoning::ReasoningEvent::AgentConnected {
                session_id: session_id.to_string(),
                agent_name: client_name,
                agent_version: client_version,
            });

            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    },
                    "resources": {
                        "subscribe": false,
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

        "resources/list" => {
            let resources = serde_json::json!([
                {
                    "uri": "memory://stats",
                    "name": "Memory Statistics",
                    "description": "Summary metrics, memory counts by type, and database size",
                    "mimeType": "application/json"
                },
                {
                    "uri": "memory://recent",
                    "name": "Recent Memories",
                    "description": "The 20 most recent memory records in the current project",
                    "mimeType": "application/json"
                }
            ]);
            Some(JsonRpcResponse::success(
                id,
                serde_json::json!({ "resources": resources }),
            ))
        }

        "resources/read" => {
            let uri = request
                .params
                .get("uri")
                .and_then(|u| u.as_str())
                .unwrap_or("");
            let content =
                match uri {
                    "memory://stats" => {
                        let stats = engine.store.stats().await.unwrap_or(
                            rememhq_core::storage::StoreStats {
                                total_memories: 0,
                                by_type: std::collections::HashMap::new(),
                                avg_importance: 0.0,
                                db_size_bytes: 0,
                            },
                        );
                        serde_json::to_string_pretty(&stats).unwrap_or_default()
                    }
                    "memory://recent" => {
                        let recent = engine
                            .list_memories(&[], None, None, 20)
                            .await
                            .unwrap_or_default();
                        serde_json::to_string_pretty(&recent).unwrap_or_default()
                    }
                    _ => format!("Resource not found: {}", uri),
                };
            Some(JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": content
                    }]
                }),
            ))
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
            Err(e) => Some(JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                }),
            )),
        },

        _ => Some(JsonRpcResponse::error(
            id,
            -32601,
            format!("Method not found: {}", request.method),
        )),
    }
}
