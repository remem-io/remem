//! CORS configuration helpers.

use tower_http::cors::CorsLayer;

pub fn create_cors_layer(cors_origin: Option<&str>) -> CorsLayer {
    match cors_origin {
        Some("*") | Some("any") => {
            tracing::warn!("CORS configured with wildcard origin ('*'). All origins permitted.");
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
        }
        Some(origins_str) => {
            let origins: Vec<axum::http::HeaderValue> = origins_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();

            if origins.is_empty() {
                tracing::warn!(
                    "REMEM_CORS_ORIGIN was set to '{}' but contained no valid header values. Falling back to default local origins.",
                    origins_str
                );
                create_default_cors_layer()
            } else {
                tracing::info!("CORS enabled for origins: {}", origins_str);
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::PATCH,
                        axum::http::Method::DELETE,
                        axum::http::Method::OPTIONS,
                    ])
                    .allow_headers(tower_http::cors::Any)
            }
        }
        None => {
            tracing::info!("CORS initialized with safe default (localhost / 127.0.0.1 origins)");
            create_default_cors_layer()
        }
    }
}

pub fn create_default_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            if let Ok(origin_str) = origin.to_str() {
                origin_str == "http://localhost"
                    || origin_str == "http://127.0.0.1"
                    || origin_str.starts_with("http://localhost:")
                    || origin_str.starts_with("http://127.0.0.1:")
            } else {
                false
            }
        }))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
}
