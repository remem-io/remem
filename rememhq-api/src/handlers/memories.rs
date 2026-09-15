//! Memory route handlers.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use std::sync::Arc;
use validator::Validate;

use crate::cursor::{decode_cursor, encode_cursor};
use crate::middleware::auth::check_auth;
use crate::models::*;
use rememhq_core::memory::types::*;
use rememhq_core::reasoning::ReasoningEngine;

type AppState = Arc<ReasoningEngine>;

/// Store a new memory.
#[utoipa::path(
    post,
    path = "/v1/memories",
    request_body = ApiStoreRequest,
    responses(
        (status = 201, description = "Memory stored successfully", body = StoreResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn store_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ApiStoreRequest>,
) -> Result<(StatusCode, Json<StoreResponse>), (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;
    if let Err(e) = req.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }

    let auto_score = req.importance.is_none();
    let memory_type = req
        .memory_type
        .and_then(|s| s.parse().ok())
        .unwrap_or(MemoryType::Fact);
    let mut record =
        MemoryRecord::new(&req.content, memory_type).with_tags(req.tags.unwrap_or_default());

    if let Some(imp) = req.importance {
        record = record.with_importance(imp);
    }
    if let Some(ttl) = req.ttl_days {
        record = record.with_ttl(ttl);
    }

    let options = crate::middleware::auth::extract_provider_options(&headers);
    let stored = engine
        .store_memory(record, auto_score, options.as_ref())
        .await
        .map_err(|e| {
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
        ("cursor" = Option<String>, Query, description = "Results offset"),
        ("filter_tags" = Option<String>, Query, description = "Comma-separated list of tags to filter by"),
        ("since" = Option<String>, Query, description = "ISO8601/RFC3339 timestamp filter"),
        ("memory_type" = Option<String>, Query, description = "Memory type filter (fact, procedure, preference, decision)")
    ),
    responses(
        (status = 200, description = "Recall results", body = PaginatedResponse<MemoryResult>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn recall_memories(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RecallQuery>,
) -> Result<Json<PaginatedResponse<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;
    if let Err(e) = q.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }

    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let since = q
        .since
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let memory_type = q.memory_type.and_then(|s| s.parse().ok());

    let offset = decode_cursor(q.cursor);
    let limit = q.limit;
    let fetch_limit = offset.saturating_add(limit);

    let options = crate::middleware::auth::extract_provider_options(&headers);
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
    let next_cursor = if paginated.len() == limit {
        Some(encode_cursor(offset.saturating_add(limit)))
    } else {
        None
    };
    Ok(Json(PaginatedResponse {
        data: paginated,
        next_cursor,
    }))
}

/// Search memories using simple vector similarity.
#[utoipa::path(
    get,
    path = "/v1/memories/search",
    params(
        ("q" = String, Query, description = "Search query string"),
        ("limit" = Option<usize>, Query, description = "Max results to return"),
        ("cursor" = Option<String>, Query, description = "Results offset"),
        ("filter_tags" = Option<String>, Query, description = "Comma-separated list of tags to filter by")
    ),
    responses(
        (status = 200, description = "Search results", body = PaginatedResponse<MemoryResult>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn search_memories(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<PaginatedResponse<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;
    if let Err(e) = q.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }

    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let offset = decode_cursor(q.cursor);
    let limit = q.limit;
    let fetch_limit = offset.saturating_add(limit);

    let options = crate::middleware::auth::extract_provider_options(&headers);
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
    let next_cursor = if paginated.len() == limit {
        Some(encode_cursor(offset.saturating_add(limit)))
    } else {
        None
    };
    Ok(Json(PaginatedResponse {
        data: paginated,
        next_cursor,
    }))
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
pub async fn update_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;
    if let Err(e) = body.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }

    let id = uuid::Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid UUID".into(),
            }),
        )
    })?;

    let options = crate::middleware::auth::extract_provider_options(&headers);
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
pub async fn apply_decay(
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

/// Compact a conversation trace to save context window tokens.
#[utoipa::path(
    post,
    path = "/v1/memories/compact",
    request_body = CompactBody,
    responses(
        (status = 200, description = "Context compacted successfully", body = CompactResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn compact_context(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CompactBody>,
) -> Result<Json<CompactResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let report = engine
        .compact_context(
            &body.conversation_text,
            body.focus_areas.as_deref(),
            crate::middleware::auth::extract_provider_options(&headers).as_ref(),
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

    Ok(Json(CompactResponse {
        compressed_context: report.compressed_context,
        original_length: report.original_length,
        compressed_length: report.compressed_length,
    }))
}

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
        (status = 200, description = "List of memories", body = PaginatedResponse<MemoryRecord>)
    )
)]
pub async fn list_memories(
    State(engine): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<PaginatedResponse<MemoryRecord>>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = q.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    let offset = decode_cursor(q.cursor);
    let filter_tags: Vec<String> = q
        .filter_tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let memory_type = q.memory_type.and_then(|t| t.parse().ok());
    let since = q.since.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc))
    });

    match engine
        .list_memories(&filter_tags, memory_type, since, q.limit)
        .await
    {
        Ok(memories) => {
            let paginated = memories
                .into_iter()
                .skip(offset)
                .take(q.limit)
                .collect::<Vec<_>>();
            let next_cursor = if paginated.len() == q.limit {
                Some(encode_cursor(offset.saturating_add(q.limit)))
            } else {
                None
            };
            Ok(Json(PaginatedResponse {
                data: paginated,
                next_cursor,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to list memories: {}", e),
            }),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/v1/memories/expire",
    responses(
        (status = 200, description = "Expired memories count")
    )
)]
pub async fn expire_memories(
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use rememhq_core::config::RememConfig;
    use rememhq_core::memory::types::{MemoryRecord, MemoryType};
    use rememhq_core::providers::mock::{MockEmbeddings, MockProvider};
    use rememhq_core::providers::EmbeddingProvider;
    use rememhq_core::reasoning::ReasoningEngine;
    use rememhq_core::storage::sqlite::SqliteStore;
    use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
    use tower::ServiceExt;

    async fn engine_with_memories(count: usize) -> Arc<ReasoningEngine> {
        let store = SqliteStore::open_in_memory().unwrap();
        let index = HNSWVectorIndex::new(768, 100);
        let provider = Arc::new(MockProvider);
        let embeddings = Arc::new(MockEmbeddings::new(768));

        for i in 0..count {
            let record = MemoryRecord::new(format!("Alice test memory {i}"), MemoryType::Fact);
            let embedding = embeddings.embed(&record.content, None).await.unwrap();
            let mut record_with_emb = record.clone();
            record_with_emb.embedding = Some(embedding.clone());
            store.insert(&record_with_emb).await.unwrap();
            index.add(record.id, &embedding).await.unwrap();
        }

        Arc::new(ReasoningEngine::new(
            RememConfig::default(),
            provider,
            embeddings,
            Arc::new(store),
            Arc::new(index),
            vec![],
        ))
    }

    fn app(engine: Arc<ReasoningEngine>) -> Router {
        Router::new()
            .route("/v1/memories/recall", get(recall_memories))
            .route("/v1/memories/search", get(search_memories))
            .with_state(engine)
    }

    // Regression test: `limit` had no upper bound on the live /recall and
    // /search handlers (only the query string `q` was validated), and
    // `fetch_limit = offset + limit` was a plain, unchecked addition. A
    // single request with an enormous attacker-controlled `limit` would
    // flow straight into the vector index with no defense — the exact
    // class of bug this crate already guards against elsewhere (see
    // rememhq-mcp's MAX_TOOL_LIMIT, and the identical check that existed
    // only in the dead code this handler was meant to replace).
    #[tokio::test]
    async fn test_oversized_recall_limit_is_rejected_with_400() {
        let engine = engine_with_memories(1).await;
        let app = app(engine);

        let req = axum::http::Request::builder()
            .uri("/v1/memories/recall?q=Alice&limit=999999999999")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_oversized_search_limit_is_rejected_with_400() {
        let engine = engine_with_memories(1).await;
        let app = app(engine);

        let req = axum::http::Request::builder()
            .uri("/v1/memories/search?q=Alice&limit=999999999999")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_recall_returns_paginated_envelope() {
        // Regression test for the SDK-side bug fixed in #123/#124: this
        // endpoint must return {"data": [...], "next_cursor": ...}, not a
        // bare JSON array, since every first-party SDK deserializes it as
        // PaginatedResponse<MemoryResult>.
        let engine = engine_with_memories(3).await;
        let app = app(engine);

        let req = axum::http::Request::builder()
            .uri("/v1/memories/recall?q=Alice&limit=2")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), 10_000).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value.get("data").is_some(), "response must have a top-level 'data' array");
        assert!(value.get("data").unwrap().is_array());
        assert_eq!(value["data"].as_array().unwrap().len(), 2);
        // 2 of 3 memories returned, so there must be a next_cursor to page further.
        assert!(value.get("next_cursor").is_some());
        assert!(!value["next_cursor"].is_null());
    }

    #[tokio::test]
    async fn test_recall_rejects_empty_query() {
        let engine = engine_with_memories(1).await;
        let app = app(engine);

        let req = axum::http::Request::builder()
            .uri("/v1/memories/recall?q=")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
