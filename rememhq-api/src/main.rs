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
use utoipa::ToSchema;

use rememhq_core::config::RememConfig;
use rememhq_core::memory::types::*;
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

#[derive(Serialize, Deserialize, ToSchema)]
struct StoreResponse {
    id: uuid::Uuid,
    importance: f32,
    tags: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, ToSchema)]
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

#[derive(Serialize, Deserialize, ToSchema)]
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

#[derive(Serialize, Deserialize, ToSchema)]
struct ConsolidateBody {
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default = "default_20")]
    limit: usize,
    #[serde(default)]
    filter_tags: Option<String>,
    since: Option<String>,
    memory_type: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct SessionResponse {
    id: String,
    project: String,
    started_at: String,
    ended_at: Option<String>,
    consolidated: bool,
    memory_count: usize,
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
#[derive(Serialize, Deserialize, ToSchema)]
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

/// Store a new memory.
#[utoipa::path(
    post,
    path = "/v1/memories",
    request_body = StoreRequest,
    responses(
        (status = 201, description = "Memory stored successfully", body = StoreResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Recall memories using guided retrieval (vector search + LLM re-ranking).
#[utoipa::path(
    get,
    path = "/v1/memories/recall",
    params(
        ("q" = String, Query, description = "Query string"),
        ("limit" = Option<usize>, Query, description = "Max results to return"),
        ("offset" = Option<usize>, Query, description = "Results offset"),
        ("filter_tags" = Option<String>, Query, description = "Comma-separated list of tags to filter by"),
        ("since" = Option<String>, Query, description = "ISO8601/RFC3339 timestamp filter"),
        ("memory_type" = Option<String>, Query, description = "Memory type filter (fact, procedure, preference, decision)")
    ),
    responses(
        (status = 200, description = "Recall results", body = Vec<MemoryResult>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Search memories using simple vector similarity.
#[utoipa::path(
    get,
    path = "/v1/memories/search",
    params(
        ("q" = String, Query, description = "Search query string"),
        ("limit" = Option<usize>, Query, description = "Max results to return"),
        ("offset" = Option<usize>, Query, description = "Results offset"),
        ("filter_tags" = Option<String>, Query, description = "Comma-separated list of tags to filter by")
    ),
    responses(
        (status = 200, description = "Search results", body = Vec<MemoryResult>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Update an existing memory's content, importance, or tags.
#[utoipa::path(
    patch,
    path = "/v1/memories/{id}",
    params(
        ("id" = String, Path, description = "UUID of the memory to update")
    ),
    request_body = UpdateBody,
    responses(
        (status = 200, description = "Memory updated successfully", body = serde_json::Value),
        (status = 400, description = "Invalid UUID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Forget a memory (delete, archive, or decay).
#[utoipa::path(
    delete,
    path = "/v1/memories/{id}",
    params(
        ("id" = String, Path, description = "UUID of the memory to forget"),
        ("mode" = Option<String>, Query, description = "Forget mode (delete, decay, archive)")
    ),
    responses(
        (status = 200, description = "Memory forgotten successfully", body = serde_json::Value),
        (status = 400, description = "Invalid UUID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Apply decay to all active memories, archiving those that fall below threshold.
#[utoipa::path(
    post,
    path = "/v1/memories/decay",
    request_body = DecayBody,
    responses(
        (status = 200, description = "Decay applied successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

/// Consolidate a raw agent session, extracting new facts, checking contradictions, and updating the knowledge graph.
#[utoipa::path(
    post,
    path = "/v1/sessions/{id}/consolidate",
    params(
        ("id" = String, Path, description = "Session ID to consolidate")
    ),
    request_body = ConsolidateBody,
    responses(
        (status = 200, description = "Session consolidated successfully", body = ConsolidationReport),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
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

// ── List Memories ───────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/v1/memories",
    params(
        ("limit" = Option<usize>, Query, description = "Max results"),
        ("filter_tags" = Option<String>, Query, description = "Comma-separated tags"),
        ("since" = Option<String>, Query, description = "RFC3339 datetime"),
        ("memory_type" = Option<String>, Query, description = "Memory type"),
    ),
    responses(
        (status = 200, description = "List of memories", body = Vec<MemoryRecord>)
    )
)]
async fn list_memories(
    State(engine): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MemoryRecord>>, (StatusCode, Json<ErrorResponse>)> {
    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let memory_type = q.memory_type.and_then(|t| t.parse().ok());
    let since = q.since.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&chrono::Utc)));

    match engine.list_memories(&filter_tags, memory_type, since, q.limit).await {
        Ok(memories) => Ok(Json(memories)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to list memories: {}", e),
            }),
        )),
    }
}

// ── Expiration ──────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/v1/memories/expire",
    responses(
        (status = 200, description = "Expired memories count")
    )
)]
async fn expire_memories(
    State(engine): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    match engine.expire_ttl().await {
        Ok(count) => Ok(Json(serde_json::json!({ "expired": count }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to expire memories: {}", e),
            }),
        )),
    }
}

// ── Sessions ────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/v1/sessions",
    responses(
        (status = 200, description = "Session created", body = SessionResponse)
    )
)]
async fn create_session(
    State(engine): State<AppState>,
) -> Result<Json<SessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id = uuid::Uuid::new_v4().to_string();
    match engine.create_session(&id).await {
        Ok(_) => {
            if let Ok(Some(record)) = engine.get_session(&id).await {
                Ok(Json(SessionResponse {
                    id: record.id,
                    project: record.project,
                    started_at: record.started_at,
                    ended_at: record.ended_at,
                    consolidated: record.consolidated,
                    memory_count: record.memory_count,
                }))
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: "Failed to fetch created session".into() })
                ))
            }
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create session: {}", e),
            }),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/v1/sessions/{id}/end",
    params(
        ("id" = String, Path, description = "Session ID to end")
    ),
    responses(
        (status = 200, description = "Session ended")
    )
)]
async fn end_session(
    State(engine): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    match engine.end_session(&id).await {
        Ok(true) => Ok(Json(serde_json::json!({ "status": "ended" }))),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "Session not found or already ended".into() }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to end session: {}", e),
            }),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/sessions",
    params(
        ("limit" = Option<usize>, Query, description = "Max results")
    ),
    responses(
        (status = 200, description = "List of sessions", body = Vec<SessionResponse>)
    )
)]
async fn list_sessions(
    State(engine): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<SessionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    match engine.list_sessions(q.limit).await {
        Ok(sessions) => {
            let res = sessions.into_iter().map(|r| SessionResponse {
                    id: r.id,
                    project: r.project,
                    started_at: r.started_at,
                    ended_at: r.ended_at,
                    consolidated: r.consolidated,
                    memory_count: r.memory_count,
            }).collect();
            Ok(Json(res))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to list sessions: {}", e),
            }),
        )),
    }
}

async fn health() -> &'static str {
    "ok"
}

const SWAGGER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>remem API Docs</title>
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" >
  <style>
    html { box-sizing: border-box; overflow-y: scroll; }
    *, *:before, *:after { box-sizing: inherit; }
    body { margin:0; background: #fafafa; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"> </script>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"> </script>
  <script>
    window.onload = function() {
      const ui = SwaggerUIBundle({
        url: "/api-docs/openapi.json",
        dom_id: '#swagger-ui',
        deepLinking: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        plugins: [
          SwaggerUIBundle.plugins.DownloadUrl
        ],
        layout: "StandaloneLayout"
      });
      window.ui = ui;
    };
  </script>
</body>
</html>"#;

async fn get_openapi_json() -> Json<serde_json::Value> {
    use utoipa::OpenApi;
    Json(serde_json::to_value(ApiDoc::openapi()).unwrap())
}

async fn swagger_ui_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(SWAGGER_HTML)
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        store_memory,
        list_memories,
        recall_memories,
        search_memories,
        update_memory,
        forget_memory,
        apply_decay,
        expire_memories,
        list_sessions,
        create_session,
        end_session,
        consolidate_session,
        routes::memories::get_memory,
        routes::memories::query_knowledge,
        routes::memories::get_entity_context,
        routes::memories::get_stats
    ),
    components(
        schemas(
            StoreRequest,
            StoreResponse,
            ErrorResponse,
            MemoryRecord,
            MemoryResult,
            MemoryType,
            UpdateBody,
            ForgetMode,
            DecayBody,
            ConsolidateBody,
            ConsolidationReport,
            Contradiction,
            KnowledgeGraphUpdate,
            SessionResponse,
            rememhq_core::storage::StoreStats
        )
    ),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::HttpBuilder::new()
                        .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                        .bearer_format("API Key")
                        .build(),
                ),
            );
        }
    }
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

    let provider = rememhq_core::providers::factory::build_reasoning_provider(&config);
    let embeddings = rememhq_core::providers::factory::build_embedding_provider(&config);

    tracing::info!(
        "Initializing ReasoningEngine with project: {}",
        args.project
    );
    tracing::info!("Using reasoning provider: {}", provider.name());
    tracing::info!("Using embedding provider (dim={})", embeddings.dimension());

    let index = Arc::new(HNSWVectorIndex::new(embeddings.dimension(), 10000));
    let _ = index.load(&config.index_path()).await;

    let engine = Arc::new(
        rememhq_core::reasoning::EngineBuilder::from_config(config.clone())
            .with_provider(provider)
            .with_embeddings(embeddings)
            .with_store(store)
            .with_index(index)
            .build()
            .await?
    );

    let rate_limit_state = Arc::new(tokio::sync::Mutex::new(
        middleware::rate_limit::RateLimiterState::new(),
    ));

    // Start background tasks
    let bg_engine = engine.clone();
    let decay_hours = config.memory.importance_decay_interval_hours as u64;
    tokio::spawn(async move {
        let decay_interval = std::time::Duration::from_secs(decay_hours * 3600);
        let mut decay_timer = tokio::time::interval(decay_interval);
        let mut ttl_timer = tokio::time::interval(std::time::Duration::from_secs(3600)); // check TTL every hour

        loop {
            tokio::select! {
                _ = decay_timer.tick() => {
                    tracing::info!("Running background memory decay...");
                    let _ = bg_engine.apply_decay(0.9).await;
                    let _ = bg_engine.save_index().await;
                }
                _ = ttl_timer.tick() => {
                    tracing::info!("Running background TTL expiration...");
                    let _ = bg_engine.expire_ttl().await;
                    let _ = bg_engine.save_index().await;
                }
            }
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api-docs/openapi.json", get(get_openapi_json))
        .route("/swagger-ui", get(swagger_ui_handler))
        .route("/swagger-ui/", get(swagger_ui_handler))
        .route("/v1/memories", post(store_memory))
        .route("/v1/memories", get(list_memories))
        .route("/v1/memories/recall", get(recall_memories))
        .route("/v1/memories/search", get(search_memories))
        .route("/v1/memories/decay", post(apply_decay))
        .route("/v1/memories/expire", post(expire_memories))
        .route("/v1/memories/{id}", get(routes::memories::get_memory))
        .route("/v1/memories/{id}", patch(update_memory))
        .route("/v1/memories/{id}", delete(forget_memory))
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions", post(create_session))
        .route("/v1/sessions/{id}/end", post(end_session))
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
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received, starting graceful shutdown...");
}
