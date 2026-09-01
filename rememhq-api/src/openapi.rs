//! OpenAPI documentation specifications and Swagger UI handler.

use axum::response::{Html, Json};
use utoipa::OpenApi;

use crate::handlers::{health::*, memories::*, sessions::*};
use crate::models::*;
use crate::routes;
use rememhq_core::memory::types::*;

pub const SWAGGER_HTML: &str = r#"<!DOCTYPE html>
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

pub async fn get_openapi_json() -> Json<serde_json::Value> {
    Json(serde_json::to_value(ApiDoc::openapi()).unwrap())
}

pub async fn swagger_ui_handler() -> Html<&'static str> {
    Html(SWAGGER_HTML)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        get_telemetry_metrics,
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
        compact_context,
        routes::memories::get_memory,
        routes::memories::query_knowledge,
        routes::memories::get_entity_context,
        routes::memories::get_stats
    ),
    components(
        schemas(
            ApiStoreRequest,
            PaginatedResponse<MemoryResult>,
            PaginatedResponse<MemoryRecord>,
            PaginatedResponse<SessionResponse>,
            StoreResponse,
            ErrorResponse,
            TelemetryResponse,
            MemoryRecord,
            MemoryResult,
            MemoryType,
            UpdateBody,
            ForgetMode,
            DecayBody,
            ConsolidateBody,
            ConsolidationReport,
            CompactBody,
            CompactResponse,
            Contradiction,
            KnowledgeGraphUpdate,
            SessionResponse,
            rememhq_core::storage::StoreStats,
            rememhq_core::telemetry::MetricsSnapshot,
            rememhq_core::providers::CostSummary,
            rememhq_core::providers::CacheStats
        )
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

pub struct SecurityAddon;

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
