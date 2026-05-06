//! remem REST API server built with Axum.
//!
//! Endpoints mirror the MCP tools:
//! - POST   /v1/memories              → mem_store
//! - GET    /v1/memories/recall       → mem_recall
//! - GET    /v1/memories/search       → mem_search
//! - PATCH  /v1/memories/:id          → mem_update
//! - DELETE /v1/memories/:id          → mem_forget
//! - POST   /v1/sessions/:id/consolidate → mem_consolidate

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

use remem_core::config::RememConfig;
use remem_core::memory::types::*;
use remem_core::providers::anthropic::AnthropicProvider;
use remem_core::providers::embeddings::OpenAIEmbeddings;
use remem_core::providers::openai::OpenAIProvider;
use remem_core::reasoning::ReasoningEngine;
use remem_core::storage::sqlite::SqliteStore;
use remem_core::storage::vector::{HNSWVectorIndex, VectorIndex};

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

    let results = engine
        .recall(&q.q, q.limit, &filter_tags, since, memory_type)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(results))
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

    let results = engine
        .search(&q.q, q.limit, &filter_tags)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(results))
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

    let report = remem_core::reasoning::consolidation::consolidate_session(
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
        .with_env_filter("remem=info,tower_http=debug")
        .init();

    let args = Args::parse();
    let config = RememConfig::load(&args.project, None)?;

    // Initialize components
    let store = Arc::new(SqliteStore::open(&config.db_path())?);
    let index = Arc::new(HNSWVectorIndex::new(768, 10000));
    let _ = index.load(&config.index_path()).await;

    let reasoning_provider_name = std::env::var("REMEM_REASONING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    let provider: Arc<dyn remem_core::providers::Provider> = match reasoning_provider_name.as_str()
    {
        "openai" => Arc::new(OpenAIProvider::new(None)?),
        "anthropic" => Arc::new(AnthropicProvider::new(None)?),
        "google" => Arc::new(remem_core::providers::google::GoogleProvider::new(None)?),
        "mock" | "local" => Arc::new(remem_core::providers::mock::MockProvider),
        _ => match std::env::var("GOOGLE_API_KEY") {
            Ok(_) => Arc::new(remem_core::providers::google::GoogleProvider::new(None)?),
            Err(_) => Arc::new(OpenAIProvider::new(None)?),
        },
    };

    let embedding_provider_name = std::env::var("REMEM_EMBEDDING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    let embeddings: Arc<dyn remem_core::providers::EmbeddingProvider> =
        match embedding_provider_name.as_str() {
            "google" => Arc::new(remem_core::providers::google::GoogleEmbeddings::new(None)?),
            "mock" => Arc::new(remem_core::providers::mock::MockEmbeddings::new(768)),
            "local" => {
                let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
                    .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
                let vocab_path = std::env::var("REMEM_LOCAL_VOCAB_PATH")
                    .unwrap_or_else(|_| "models/vocab.txt".to_string());
                Arc::new(remem_core::providers::local::LocalEmbeddings::new(
                    &model_path,
                    &vocab_path,
                )?)
            }
            _ => match std::env::var("GOOGLE_API_KEY") {
                Ok(_) => Arc::new(remem_core::providers::google::GoogleEmbeddings::new(None)?),
                Err(_) => Arc::new(OpenAIEmbeddings::new(None, Some(768))?),
            },
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

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/memories", post(store_memory))
        .route("/v1/memories/recall", get(recall_memories))
        .route("/v1/memories/search", get(search_memories))
        .route("/v1/memories/decay", post(apply_decay))
        .route("/v1/memories/{id}", patch(update_memory))
        .route("/v1/memories/{id}", delete(forget_memory))
        .route("/v1/sessions/{id}/consolidate", post(consolidate_session))
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
