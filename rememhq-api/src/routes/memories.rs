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

use rememhq_core::memory::types::*;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::MemoryStore;

use crate::middleware::auth::check_auth;

type AppState = Arc<ReasoningEngine>;

// --- Request/Response types ---

#[derive(serde::Serialize)]
pub struct StoreResponse {
    pub id: uuid::Uuid,
    pub importance: f32,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize)]
pub struct RecallQuery {
    pub q: String,
    #[serde(default = "default_8")]
    pub limit: usize,
    pub filter_tags: Option<String>,
    pub since: Option<String>,
    pub memory_type: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_20")]
    pub limit: usize,
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

    let stored = engine.store_memory(record, auto_score).await.map_err(|e| {
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

/// GET /v1/memories/:id — fetch a single memory by ID.
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

/// GET /v1/knowledge/entity/:name — get all knowledge triples for an entity.
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

/// GET /v1/knowledge — query knowledge graph with optional filters.
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

/// GET /v1/stats — get database statistics.
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
