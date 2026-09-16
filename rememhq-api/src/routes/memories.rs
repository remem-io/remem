//! Memory route handlers — fetch-by-id, knowledge graph, and stats.
//!
//! store/recall/search/update/forget for memories live in
//! `crate::handlers::memories` (the ones actually wired into
//! `router.rs`) — not here. This module used to duplicate all of
//! those too, but that copy had drifted out of sync with the live
//! one (no upper bound on `limit`, see #137) and was never reachable
//! through any route, so it's been removed rather than kept in sync
//! by hand going forward.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;

use rememhq_core::memory::types::*;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::{MemoryStore, StoreStats};

use crate::middleware::auth::check_auth;

type AppState = Arc<ReasoningEngine>;

pub use crate::models::ErrorResponse;

// --- Handlers ---

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
    use rememhq_core::providers::mock::{MockEmbeddings, MockProvider};
    use rememhq_core::storage::sqlite::SqliteStore;
    use rememhq_core::storage::vector::HNSWVectorIndex;
    use std::sync::Arc;
    use tower::ServiceExt;

    // These 4 handlers had no tests at all before this — the file's old
    // test module (removed alongside the dead handlers it exercised)
    // only ever covered recall_memories/search_memories, which were never
    // actually routed. See #137 and the module doc comment above.

    fn empty_engine() -> Arc<ReasoningEngine> {
        let store = SqliteStore::open_in_memory().unwrap();
        let index = HNSWVectorIndex::new(768, 100);
        Arc::new(ReasoningEngine::new(
            RememConfig::default(),
            Arc::new(MockProvider),
            Arc::new(MockEmbeddings::new(768)),
            Arc::new(store),
            Arc::new(index),
            vec![],
        ))
    }

    fn app(engine: Arc<ReasoningEngine>) -> Router {
        Router::new()
            .route("/v1/memories/{id}", get(get_memory))
            .route("/v1/stats", get(get_stats))
            .route("/v1/knowledge", get(query_knowledge))
            .with_state(engine)
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_get_memory_invalid_uuid_is_400() {
        let _guard = crate::middleware::auth::tests::ENV_TEST_LOCK
            .lock()
            .unwrap();
        std::env::remove_var("REMEM_API_KEY");

        let req = axum::http::Request::builder()
            .uri("/v1/memories/not-a-uuid")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app(empty_engine()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_get_memory_missing_is_404() {
        let _guard = crate::middleware::auth::tests::ENV_TEST_LOCK
            .lock()
            .unwrap();
        std::env::remove_var("REMEM_API_KEY");

        let req = axum::http::Request::builder()
            .uri(format!("/v1/memories/{}", uuid::Uuid::new_v4()))
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app(empty_engine()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_get_stats_on_empty_store() {
        let _guard = crate::middleware::auth::tests::ENV_TEST_LOCK
            .lock()
            .unwrap();
        std::env::remove_var("REMEM_API_KEY");

        let req = axum::http::Request::builder()
            .uri("/v1/stats")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app(empty_engine()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_query_knowledge_with_no_matches_returns_empty_array() {
        let _guard = crate::middleware::auth::tests::ENV_TEST_LOCK
            .lock()
            .unwrap();
        std::env::remove_var("REMEM_API_KEY");

        let req = axum::http::Request::builder()
            .uri("/v1/knowledge?subject=nobody")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app(empty_engine()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), 10_000).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 0);
    }
}
