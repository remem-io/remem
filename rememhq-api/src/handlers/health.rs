//! Healthcheck and Telemetry handlers.

use axum::{extract::State, http::StatusCode, response::Json};
use std::sync::Arc;

use crate::models::TelemetryResponse;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::MemoryStore;

type AppState = Arc<ReasoningEngine>;

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = serde_json::Value),
        (status = 503, description = "Service unavailable", body = serde_json::Value)
    )
)]
pub async fn health(
    State(engine): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match engine.store.stats().await {
        Ok(_) => Ok(Json(serde_json::json!({
            "status": "ok",
            "db": "connected",
            "in_flight_requests": engine.pool.in_flight(),
            "cache_entries": engine.cache.stats().total_entries
        }))),
        Err(_) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "error", "db": "disconnected" })),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/telemetry/metrics",
    responses(
        (status = 200, description = "Telemetry, performance metrics, and cost metering", body = TelemetryResponse)
    )
)]
pub async fn get_telemetry_metrics(State(engine): State<AppState>) -> Json<TelemetryResponse> {
    let sessions_count = engine
        .list_sessions(1000)
        .await
        .map(|s| s.len())
        .unwrap_or(0);
    Json(TelemetryResponse {
        metrics: engine.metrics.snapshot(sessions_count),
        cost_meter: engine.pool.cost_tracker.summary(),
        cache_stats: engine.cache.stats(),
    })
}

/// Standard Prometheus scrape endpoint returning text/plain exposition format.
pub async fn get_prometheus_metrics(
    State(engine): State<AppState>,
) -> (axum::http::HeaderMap, String) {
    let sessions_count = engine
        .list_sessions(1000)
        .await
        .map(|s| s.len())
        .unwrap_or(0);
    let snapshot = engine.metrics.snapshot(sessions_count);
    let stats = engine.store.stats().await.ok();
    let total_memories = stats.map(|s| s.total_memories);
    let cost = engine.pool.cost_tracker.summary();
    let cache = engine.cache.stats();

    let extra = rememhq_core::telemetry::PrometheusExtraMetrics {
        total_memories,
        total_tokens: Some(cost.total_tokens),
        total_cost_usd: Some(cost.estimated_cost_usd),
        cache_hits: Some(cache.hits),
        cache_misses: Some(cache.misses),
    };

    let text = engine
        .metrics
        .render_prometheus_full(&snapshot, Some(&extra));

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    (headers, text)
}
