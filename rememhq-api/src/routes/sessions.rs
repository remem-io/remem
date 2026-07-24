//! Session route handlers — consolidation.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;

use rememhq_core::memory::types::ConsolidationReport;
use rememhq_core::reasoning::ReasoningEngine;

use crate::middleware::auth::{check_auth, extract_provider_options};
use crate::routes::memories::ErrorResponse;

#[allow(dead_code)]
type AppState = Arc<ReasoningEngine>;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct ConsolidateBody {
    pub model: Option<String>,
}

#[allow(dead_code)]
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
        extract_provider_options(&headers).as_ref(),
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
