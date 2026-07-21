//! Memory route handlers — store, recall, search, update, forget.

#![allow(dead_code)]

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

use rememhq_core::memory::types::*;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::{MemoryStore, StoreStats};

use crate::middleware::auth::{check_auth, extract_provider_options};

type AppState = Arc<ReasoningEngine>;

// --- Request/Response types ---

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct StoreResponse {
    pub id: uuid::Uuid,
    pub importance: f32,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize)]
pub struct RecallQuery {
    pub q: String,
    #[serde(default = "default_8")]
    pub limit: usize,
    pub offset: Option<usize>,
    pub filter_tags: Option<String>,
    pub since: Option<String>,
    pub memory_type: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_20")]
    pub limit: usize,
    pub offset: Option<usize>,
    pub filter_tags: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct ForgetQuery {
    #[serde(default = "default_delete")]
    pub mode: String,
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

/// Upper bound on `limit + offset` accepted by the recall/search endpoints.
///
/// Without a cap, `offset + limit` (both taken directly from the query
/// string) is passed straight through to the vector index search and,
/// downstream, to an FFI call into the native HNSW library — so a single
/// request with an enormous `limit` (no auth required if `REMEM_API_KEY`
/// isn't set) could force a huge allocation/search there. It also let
/// `offset + limit` overflow `usize` for extreme inputs. 1000 is generous
/// relative to the defaults (8 and 20) while still ruling out abuse.
const MAX_FETCH_LIMIT: usize = 1000;

/// Validates `limit`/`offset` from a query string and returns the safe,
/// overflow-free `offset + limit` to fetch, or a 400 error describing why
/// the request was rejected.
fn validate_fetch_limit(
    limit: usize,
    offset: usize,
) -> Result<usize, (StatusCode, Json<ErrorResponse>)> {
    let fetch_limit = offset.saturating_add(limit);
    if fetch_limit > MAX_FETCH_LIMIT {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "limit + offset ({fetch_limit}) exceeds the maximum of {MAX_FETCH_LIMIT}"
                ),
            }),
        ));
    }
    Ok(fetch_limit)
}

// --- Handlers ---

pub async fn store_memory(
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

    let options = extract_provider_options(&headers);
    let stored = engine
        .store_memory(record, auto_score, options.as_ref())
        .await
        .map_err(|e| {
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

pub async fn recall_memories(
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
    let fetch_limit = validate_fetch_limit(limit, offset)?;

    let options = extract_provider_options(&headers);
    let results = engine
        .recall(
            &q.q,
            fetch_limit,
            &filter_tags,
            since,
            memory_type,
            options.as_ref(),
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

    let paginated = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(Json(paginated))
}

pub async fn search_memories(
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
    let fetch_limit = validate_fetch_limit(limit, offset)?;

    let options = extract_provider_options(&headers);
    let results = engine
        .search(&q.q, fetch_limit, &filter_tags, options.as_ref())
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

pub async fn update_memory(
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

    let options = extract_provider_options(&headers);
    let updated = engine
        .update_memory(
            id,
            body.content,
            body.importance,
            body.tags,
            options.as_ref(),
        )
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

pub async fn forget_memory(
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

/// Fetch a single memory by its UUID.
#[utoipa::path(
    get,
    path = "/v1/memories/{id}",
    params(
        ("id" = String, Path, description = "UUID of the memory to fetch")
    ),
    responses(
        (status = 200, description = "Memory fetched successfully", body = MemoryRecord),
        (status = 400, description = "Invalid UUID"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Memory not found", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
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

    let record = engine
        .store
        .get(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Memory not found: {}", id),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "id": record.id,
        "content": record.content,
        "importance": record.importance,
        "tags": record.tags,
        "memory_type": record.memory_type,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "decay_score": record.decay_score,
        "source_session": record.source_session,
        "ttl_days": record.ttl_days,
    })))
}

// --- Knowledge Graph types ---

#[derive(Deserialize)]
pub struct KnowledgeQuery {
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<String>,
}

/// Get all knowledge graph triples associated with a specific entity name.
#[utoipa::path(
    get,
    path = "/v1/knowledge/entity/{name}",
    params(
        ("name" = String, Path, description = "Entity name to retrieve context for")
    ),
    responses(
        (status = 200, description = "Knowledge graph triples", body = Vec<KnowledgeGraphUpdate>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_entity_context(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(entity): Path<String>,
) -> Result<
    Json<Vec<rememhq_core::memory::types::KnowledgeGraphUpdate>>,
    (StatusCode, Json<ErrorResponse>),
> {
    check_auth(&headers)?;

    let triples = engine.get_entity_context(&entity).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(triples))
}

/// Query the knowledge graph with optional subject, predicate, or object filters.
#[utoipa::path(
    get,
    path = "/v1/knowledge",
    params(
        ("subject" = Option<String>, Query, description = "Subject filter"),
        ("predicate" = Option<String>, Query, description = "Predicate filter"),
        ("object" = Option<String>, Query, description = "Object filter")
    ),
    responses(
        (status = 200, description = "Matching knowledge graph triples", body = Vec<KnowledgeGraphUpdate>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn query_knowledge(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<KnowledgeQuery>,
) -> Result<
    Json<Vec<rememhq_core::memory::types::KnowledgeGraphUpdate>>,
    (StatusCode, Json<ErrorResponse>),
> {
    check_auth(&headers)?;

    let triples = engine
        .query_knowledge(
            q.subject.as_deref(),
            q.predicate.as_deref(),
            q.object.as_deref(),
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

    Ok(Json(triples))
}

/// Get database and memory usage statistics.
#[utoipa::path(
    get,
    path = "/v1/stats",
    responses(
        (status = 200, description = "Database statistics", body = StoreStats),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_stats(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<rememhq_core::storage::StoreStats>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let stats = engine.store.stats().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use rememhq_core::config::RememConfig;
    use rememhq_core::memory::types::{MemoryRecord, MemoryType};
    use rememhq_core::providers::mock::{MockEmbeddings, MockProvider};
    use rememhq_core::providers::EmbeddingProvider;
    use rememhq_core::storage::sqlite::SqliteStore;
    use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_pagination_offset_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        let index = HNSWVectorIndex::new(768, 100);
        let provider = Arc::new(MockProvider);
        let embeddings = Arc::new(MockEmbeddings::new(768));

        // Insert 5 test memories
        for i in 0..5 {
            let record = MemoryRecord::new(format!("Alice test memory {}", i), MemoryType::Fact);
            let embedding = embeddings.embed(&record.content, None).await.unwrap();

            let mut record_with_emb = record.clone();
            record_with_emb.embedding = Some(embedding.clone());
            store.insert(&record_with_emb).await.unwrap();
            index.add(record.id, &embedding).await.unwrap();
        }

        let config = RememConfig::default();
        let engine = Arc::new(ReasoningEngine::new(
            config,
            provider,
            embeddings,
            Arc::new(store),
            Arc::new(index),
            vec![],
        ));

        let app = Router::new()
            .route("/v1/memories/recall", get(recall_memories))
            .route("/v1/memories/search", get(search_memories))
            .with_state(engine);

        // Test search pagination: limit = 2, offset = 0
        let req = axum::http::Request::builder()
            .uri("/v1/memories/search?q=Alice&limit=2&offset=0")
            .body(axum::body::Body::empty())
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), 10000).await.unwrap();
        let memories: Vec<MemoryResult> = serde_json::from_slice(&body).unwrap();
        assert_eq!(memories.len(), 2);

        // Test search pagination: limit = 2, offset = 2
        let req2 = axum::http::Request::builder()
            .uri("/v1/memories/search?q=Alice&limit=2&offset=2")
            .body(axum::body::Body::empty())
            .unwrap();

        let res2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);

        let body2 = axum::body::to_bytes(res2.into_body(), 10000).await.unwrap();
        let memories2: Vec<MemoryResult> = serde_json::from_slice(&body2).unwrap();
        assert_eq!(memories2.len(), 2);

        // Assert that the memories in page 2 are different from page 1
        assert_ne!(memories[0].id, memories2[0].id);
        assert_ne!(memories[1].id, memories2[1].id);

        // Test recall pagination: limit = 1, offset = 4
        let req3 = axum::http::Request::builder()
            .uri("/v1/memories/recall?q=Alice&limit=1&offset=4")
            .body(axum::body::Body::empty())
            .unwrap();

        let res3 = app.clone().oneshot(req3).await.unwrap();
        assert_eq!(res3.status(), StatusCode::OK);

        let body3 = axum::body::to_bytes(res3.into_body(), 10000).await.unwrap();
        let memories3: Vec<MemoryResult> = serde_json::from_slice(&body3).unwrap();
        assert_eq!(memories3.len(), 1);
    }

    #[test]
    fn test_validate_fetch_limit_within_bounds() {
        assert_eq!(validate_fetch_limit(20, 0).unwrap(), 20);
        assert_eq!(validate_fetch_limit(500, 500).unwrap(), 1000);
    }

    #[test]
    fn test_validate_fetch_limit_rejects_over_max() {
        assert!(validate_fetch_limit(1001, 0).is_err());
        assert!(validate_fetch_limit(500, 501).is_err());
    }

    #[test]
    fn test_validate_fetch_limit_does_not_panic_on_overflow() {
        // Regression test: `offset + limit` used to be a plain `+`, which
        // panics on overflow in debug/test builds (and silently wraps in
        // release builds) for extreme, attacker-controlled query params.
        // This must return a clean 400 instead of panicking either way.
        let result = validate_fetch_limit(usize::MAX, usize::MAX);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_oversized_limit_is_rejected_with_400() {
        // Relies on REMEM_API_KEY being unset (dev-mode auth). Guard against
        // the auth middleware's own tests concurrently toggling that same
        // process-wide env var — see crate::middleware::auth::tests.
        let _guard = crate::middleware::auth::tests::ENV_TEST_LOCK
            .lock()
            .unwrap();
        std::env::remove_var("REMEM_API_KEY");

        let store = SqliteStore::open_in_memory().unwrap();
        let index = HNSWVectorIndex::new(768, 100);
        let provider = Arc::new(MockProvider);
        let embeddings = Arc::new(MockEmbeddings::new(768));
        let config = RememConfig::default();
        let engine = Arc::new(ReasoningEngine::new(
            config,
            provider,
            embeddings,
            Arc::new(store),
            Arc::new(index),
            vec![],
        ));

        let app = Router::new()
            .route("/v1/memories/recall", get(recall_memories))
            .route("/v1/memories/search", get(search_memories))
            .with_state(engine);

        // A single request asking for an enormous number of results must be
        // rejected before it ever reaches the vector index, not silently
        // truncated or (worse) allowed through to a huge/overflowing search.
        let req = axum::http::Request::builder()
            .uri("/v1/memories/search?q=Alice&limit=999999999999")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let req = axum::http::Request::builder()
            .uri("/v1/memories/recall?q=Alice&limit=100&offset=999999999999")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
