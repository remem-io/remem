//! Router assembly and middleware configuration for rememhq-api.

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post},
    Router,
};
use std::sync::Arc;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::cors::create_cors_layer;
use crate::handlers::{health, memories, sessions};
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimiterState};
use crate::openapi::{get_openapi_json, swagger_ui_handler};
use crate::routes;
use rememhq_core::reasoning::ReasoningEngine;

pub fn build_app(
    engine: Arc<ReasoningEngine>,
    rate_limit_state: Arc<tokio::sync::Mutex<RateLimiterState>>,
    cors_origin: Option<&str>,
) -> Router {
    let cors_layer = create_cors_layer(cors_origin);

    Router::new()
        .route("/health", get(health::health))
        .route("/metrics", get(health::get_prometheus_metrics))
        .route("/v1/telemetry/metrics", get(health::get_telemetry_metrics))
        .route("/api-docs/openapi.json", get(get_openapi_json))
        .route("/swagger-ui", get(swagger_ui_handler))
        .route("/swagger-ui/", get(swagger_ui_handler))
        .route("/v1/memories", post(memories::store_memory))
        .route("/v1/memories", get(memories::list_memories))
        .route("/v1/memories/recall", get(memories::recall_memories))
        .route("/v1/memories/search", get(memories::search_memories))
        .route("/v1/memories/decay", post(memories::apply_decay))
        .route("/v1/memories/expire", post(memories::expire_memories))
        .route("/v1/memories/compact", post(memories::compact_context))
        .route("/v1/memories/{id}", get(routes::memories::get_memory))
        .route("/v1/memories/{id}", patch(memories::update_memory))
        .route("/v1/memories/{id}", delete(memories::forget_memory))
        .route("/v1/sessions", get(sessions::list_sessions))
        .route("/v1/sessions", post(sessions::create_session))
        .route("/v1/sessions/{id}/end", post(sessions::end_session))
        .route(
            "/v1/sessions/{id}/consolidate",
            post(sessions::consolidate_session),
        )
        .route("/v1/knowledge", get(routes::memories::query_knowledge))
        .route(
            "/v1/knowledge/entity/{name}",
            get(routes::memories::get_entity_context),
        )
        .route("/v1/stats", get(routes::memories::get_stats))
        .route(
            "/v1/memory_stores",
            post(routes::memory_stores::create_store),
        )
        .route("/v1/memory_stores", get(routes::memory_stores::list_stores))
        .route(
            "/v1/memory_stores/{store_id}",
            get(routes::memory_stores::get_store),
        )
        .route(
            "/v1/memory_stores/{store_id}/archive",
            post(routes::memory_stores::archive_store),
        )
        .route(
            "/v1/memory_stores/{store_id}/memories",
            get(routes::memory_stores::list_store_memories),
        )
        .route(
            "/v1/memory_stores/{store_id}/memories",
            post(routes::memory_stores::create_store_memory),
        )
        .route(
            "/v1/memory_stores/{store_id}/memories/{path_or_id}",
            get(routes::memory_stores::get_store_memory),
        )
        .route(
            "/v1/memory_stores/{store_id}/memories/{path_or_id}",
            post(routes::memory_stores::update_store_memory),
        )
        .route(
            "/v1/memory_stores/{store_id}/memories/{path_or_id}/versions",
            get(routes::memory_stores::list_memory_versions),
        )
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(axum::middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .with_state(engine)
}
