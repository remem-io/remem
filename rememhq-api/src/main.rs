//! remem REST API server built with Axum.
//!
//! Endpoints mirror the MCP tools:
//! - POST   /v1/memories              → mem_store
//! - GET    /v1/memories/recall       → mem_recall
//! - GET    /v1/memories/search       → mem_search
//! - GET    /v1/memories/:id          → get_memory
//! - PATCH  /v1/memories/:id          → mem_update
//! - DELETE /v1/memories/:id          → mem_forget
//! - POST   /v1/sessions/:id/consolidate → mem_consolidate
//! - GET    /v1/knowledge             → query_knowledge
//! - GET    /v1/knowledge/entity/:name → get_entity_context
//! - GET    /v1/stats                 → get_stats

mod middleware;
mod routes;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, patch, post},
    Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use rememhq_core::config::RememConfig;
use rememhq_core::memory::types::*;
use rememhq_core::providers::anthropic::AnthropicProvider;
use rememhq_core::providers::embeddings::OpenAIEmbeddings;
use rememhq_core::providers::openai::OpenAIProvider;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};

type AppState = Arc<ReasoningEngine>;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "7474")]
    port: u16,
    #[arg(long, default_value = "default")]
    project: String,
}

// --- Response types ---

#[derive(Serialize)]
struct StoreResponse {
    id: uuid::Uuid,
    importance: f32,
    tags: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct RecallQuery {
    q: String,
    #[serde(default = "default_8")]
    limit: usize,
    offset: Option<usize>,
    #[serde(default)]
    filter_tags: Option<String>,
    since: Option<String>,
    memory_type: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_20")]
    limit: usize,
    offset: Option<usize>,
    #[serde(default)]
    filter_tags: Option<String>,
}

#[derive(Deserialize)]
struct UpdateBody {
    content: Option<String>,
    importance: Option<f32>,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ForgetQuery {
    #[serde(default = "default_delete")]
    mode: String,
}

#[derive(Deserialize)]
struct ConsolidateBody {
    #[serde(default)]
    model: Option<String>,
}

fn default_8() -> usize {
    8
}
fn default_20() -> usize {
    20
}
fn default_delete() -> String {
    "delete".into()
}
#[derive(Deserialize)]
struct DecayBody {
    #[serde(default = "default_factor")]
    factor: f32,
}
fn default_factor() -> f32 {
    0.9
}

// --- Auth middleware ---

fn check_auth(headers: &HeaderMap) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Ok(expected) = std::env::var("REMEM_API_KEY") {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");

        if provided != expected {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Invalid API key".into(),
                }),
            ));
        }
    }
    Ok(())
}

// --- Handlers ---

async fn store_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StoreRequest>,
) -> Result<(StatusCode, Json<StoreResponse>), (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let auto_score = req.importance.is_none();
    let mut record = MemoryRecord::new(&req.content, req.memory_type).with_tags(req.tags);

    if let Some(imp) = req.importance {
        record = record.with_importance(imp);
    }
    if let Some(ttl) = req.ttl_days {
        record = record.with_ttl(ttl);
    }

    let stored = engine.store_memory(record, auto_score).await.map_err(|e| {
        tracing::error!("store_memory failed: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(StoreResponse {
            id: stored.id,
            importance: stored.importance,
            tags: stored.tags,
            created_at: stored.created_at,
        }),
    ))
}

async fn recall_memories(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RecallQuery>,
) -> Result<Json<Vec<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let since = q
        .since
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let memory_type = q.memory_type.and_then(|s| s.parse().ok());

    let offset = q.offset.unwrap_or(0);
    let limit = q.limit;
    let fetch_limit = offset + limit;

    let results = engine
        .recall(&q.q, fetch_limit, &filter_tags, since, memory_type)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    let paginated = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(Json(paginated))
}

async fn search_memories(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let offset = q.offset.unwrap_or(0);
    let limit = q.limit;
    let fetch_limit = offset + limit;

    let results = engine
        .search(&q.q, fetch_limit, &filter_tags)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    let paginated = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(Json(paginated))
}

async fn update_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let id = uuid::Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid UUID".into(),
            }),
        )
    })?;

    let updated = engine
        .update_memory(id, body.content, body.importance, body.tags)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "id": updated.id,
        "content": updated.content,
        "importance": updated.importance,
        "tags": updated.tags,
        "updated_at": updated.updated_at,
    })))
}

async fn forget_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ForgetQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let id = uuid::Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid UUID".into(),
            }),
        )
    })?;

    let mode: ForgetMode =
        serde_json::from_value(serde_json::json!(q.mode)).unwrap_or(ForgetMode::Delete);

    let success = engine.forget(id, mode).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({ "success": success })))
}

async fn apply_decay(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DecayBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let archived_count = engine.apply_decay(body.factor).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "archived_count": archived_count,
        "factor": body.factor
    })))
}

async fn consolidate_session(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<ConsolidateBody>,
) -> Result<Json<ConsolidationReport>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let model = body
        .model
        .unwrap_or_else(|| engine.config.reasoning.reasoning_model.clone());

    let report = rememhq_core::reasoning::consolidation::consolidate_session(
        &*engine.provider,
        &*engine.embeddings,
        &engine.store,
        engine.index.as_ref(),
        &session_id,
        &model,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(report))
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter("rememhq=info,tower_http=debug")
        .init();

    let args = Args::parse();
    let config = RememConfig::load(&args.project, None)?;

    // Initialize components
    let store = Arc::new(SqliteStore::open(&config.db_path())?);
    let index = Arc::new(HNSWVectorIndex::new(768, 10000));
    let _ = index.load(&config.index_path()).await;

    let reasoning_provider_name = std::env::var("REMEM_REASONING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    let provider: Arc<dyn rememhq_core::providers::Provider> = match reasoning_provider_name
        .as_str()
    {
        "openai" => match OpenAIProvider::new(None) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize configured OpenAI provider: {}. Attempting fallback...",
                    e
                );
                match AnthropicProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => match rememhq_core::providers::google::GoogleProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => {
                            tracing::warn!("No valid cloud reasoning keys found. Falling back to MockProvider.");
                            Arc::new(rememhq_core::providers::mock::MockProvider)
                        }
                    },
                }
            }
        },
        "anthropic" => match AnthropicProvider::new(None) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                tracing::warn!("Failed to initialize configured Anthropic provider: {}. Attempting fallback...", e);
                match OpenAIProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => match rememhq_core::providers::google::GoogleProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => {
                            tracing::warn!("No valid cloud reasoning keys found. Falling back to MockProvider.");
                            Arc::new(rememhq_core::providers::mock::MockProvider)
                        }
                    },
                }
            }
        },
        "google" => {
            match rememhq_core::providers::google::GoogleProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    tracing::warn!("Failed to initialize configured Google provider: {}. Attempting fallback...", e);
                    match AnthropicProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => match OpenAIProvider::new(None) {
                            Ok(p) => Arc::new(p),
                            Err(_) => {
                                tracing::warn!("No valid cloud reasoning keys found. Falling back to MockProvider.");
                                Arc::new(rememhq_core::providers::mock::MockProvider)
                            }
                        },
                    }
                }
            }
        }
        "mock" | "local" => Arc::new(rememhq_core::providers::mock::MockProvider),
        _ => {
            // Auto-detect based on env vars
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                match AnthropicProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                }
            } else if std::env::var("OPENAI_API_KEY").is_ok() {
                match OpenAIProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                }
            } else if std::env::var("GOOGLE_API_KEY").is_ok() {
                match rememhq_core::providers::google::GoogleProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                }
            } else {
                tracing::warn!("No reasoning API keys set. Falling back to MockProvider.");
                Arc::new(rememhq_core::providers::mock::MockProvider)
            }
        }
    };

    let embedding_provider_name = std::env::var("REMEM_EMBEDDING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    let embeddings: Arc<dyn rememhq_core::providers::EmbeddingProvider> =
        match embedding_provider_name.as_str() {
            "google" => match rememhq_core::providers::google::GoogleEmbeddings::new(None) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    tracing::warn!(
                        "Failed to initialize Google embeddings: {}. Attempting fallback...",
                        e
                    );
                    if std::env::var("OPENAI_API_KEY").is_ok() {
                        Arc::new(OpenAIEmbeddings::new(None, Some(768))?)
                    } else {
                        tracing::warn!("Falling back to MockEmbeddings.");
                        Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                    }
                }
            },
            "mock" => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
            "local" => {
                let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
                    .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
                let vocab_path = std::env::var("REMEM_LOCAL_VOCAB_PATH")
                    .unwrap_or_else(|_| "models/vocab.txt".to_string());
                match rememhq_core::providers::local::LocalEmbeddings::new(&model_path, &vocab_path)
                {
                    Ok(p) => Arc::new(p),
                    Err(e) => {
                        tracing::warn!("Failed to initialize Local embeddings: {}. Falling back to MockEmbeddings.", e);
                        Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                    }
                }
            }
            _ => {
                // Auto-detect based on env vars
                if std::env::var("OPENAI_API_KEY").is_ok() {
                    match OpenAIEmbeddings::new(None, Some(768)) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
                    }
                } else if std::env::var("GOOGLE_API_KEY").is_ok() {
                    match rememhq_core::providers::google::GoogleEmbeddings::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
                    }
                } else {
                    // Check if local model files exist
                    let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
                        .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
                    let vocab_path = std::env::var("REMEM_LOCAL_VOCAB_PATH")
                        .unwrap_or_else(|_| "models/vocab.txt".to_string());
                    if std::path::Path::new(&model_path).exists()
                        && std::path::Path::new(&vocab_path).exists()
                    {
                        match rememhq_core::providers::local::LocalEmbeddings::new(
                            &model_path,
                            &vocab_path,
                        ) {
                            Ok(p) => Arc::new(p),
                            Err(_) => {
                                Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                            }
                        }
                    } else {
                        tracing::warn!("No cloud API keys or local model files found for embeddings. Falling back to MockEmbeddings.");
                        Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                    }
                }
            }
        };

    tracing::info!(
        "Initializing ReasoningEngine with project: {}",
        args.project
    );
    tracing::info!("Using reasoning provider: {}", reasoning_provider_name);
    tracing::info!("Using embedding provider: {}", embedding_provider_name);
    let engine = Arc::new(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index,
    ));

    let rate_limit_state = Arc::new(tokio::sync::Mutex::new(
        middleware::rate_limit::RateLimiterState::new(),
    ));

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/memories", post(store_memory))
        .route("/v1/memories/recall", get(recall_memories))
        .route("/v1/memories/search", get(search_memories))
        .route("/v1/memories/decay", post(apply_decay))
        .route("/v1/memories/{id}", get(routes::memories::get_memory))
        .route("/v1/memories/{id}", patch(update_memory))
        .route("/v1/memories/{id}", delete(forget_memory))
        .route("/v1/sessions/{id}/consolidate", post(consolidate_session))
        .route("/v1/knowledge", get(routes::memories::query_knowledge))
        .route(
            "/v1/knowledge/entity/{name}",
            get(routes::memories::get_entity_context),
        )
        .route("/v1/stats", get(routes::memories::get_stats))
        .layer(axum::middleware::from_fn_with_state(
            rate_limit_state,
            middleware::rate_limit::rate_limit_middleware,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .with_state(engine);

    let addr = format!("0.0.0.0:{}", args.port);
    tracing::info!("remem REST API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
