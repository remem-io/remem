//! Session route handlers.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use std::sync::Arc;
use validator::Validate;

use crate::cursor::{decode_cursor, encode_cursor};
use crate::middleware::auth::check_auth;
use crate::models::{
    ConsolidateBody, ErrorResponse, ListQuery, PaginatedResponse, SessionResponse,
};
use rememhq_core::memory::types::ConsolidationReport;
use rememhq_core::reasoning::ReasoningEngine;

type AppState = Arc<ReasoningEngine>;

#[utoipa::path(
    post,
    path = "/v1/sessions",
    responses(
        (status = 200, description = "Session created", body = SessionResponse)
    )
)]
pub async fn create_session(
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
                    Json(ErrorResponse {
                        error: "Failed to fetch created session".into(),
                    }),
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
pub async fn end_session(
    State(engine): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    match engine.end_session(&id).await {
        Ok(true) => Ok(Json(serde_json::json!({ "status": "ended" }))),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Session not found or already ended".into(),
            }),
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
        (status = 200, description = "List of sessions", body = PaginatedResponse<SessionResponse>)
    )
)]
pub async fn list_sessions(
    State(engine): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<PaginatedResponse<SessionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = q.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    let offset = decode_cursor(q.cursor);
    match engine.list_sessions(offset + q.limit).await {
        Ok(sessions) => {
            let paginated = sessions
                .into_iter()
                .skip(offset)
                .take(q.limit)
                .map(|r| SessionResponse {
                    id: r.id,
                    project: r.project,
                    started_at: r.started_at,
                    ended_at: r.ended_at,
                    consolidated: r.consolidated,
                    memory_count: r.memory_count,
                })
                .collect::<Vec<_>>();
            let next_cursor = if paginated.len() == q.limit {
                Some(encode_cursor(offset + q.limit))
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
                error: format!("Failed to list sessions: {}", e),
            }),
        )),
    }
}

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
pub async fn consolidate_session(
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

    Ok(Json(report))
}
