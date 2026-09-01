//! REST API Request and Response DTO models.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct ApiStoreRequest {
    #[validate(length(min = 1, message = "Content cannot be empty"))]
    pub content: String,

    #[validate(range(
        min = 1.0,
        max = 10.0,
        message = "Importance must be between 1.0 and 10.0"
    ))]
    pub importance: Option<f32>,

    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
    pub ttl_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StoreResponse {
    pub id: uuid::Uuid,
    pub importance: f32,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct RecallQuery {
    #[validate(length(min = 1))]
    pub q: String,
    #[serde(default = "default_8")]
    pub limit: usize,
    pub cursor: Option<String>,
    #[serde(default)]
    pub filter_tags: Option<String>,
    pub since: Option<String>,
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SearchQuery {
    #[validate(length(min = 1))]
    pub q: String,
    #[serde(default = "default_20")]
    pub limit: usize,
    pub cursor: Option<String>,
    #[serde(default)]
    pub filter_tags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateBody {
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgetQuery {
    #[serde(default = "default_delete")]
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConsolidateBody {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactBody {
    pub conversation_text: String,
    #[serde(default)]
    pub focus_areas: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompactResponse {
    pub compressed_context: String,
    pub original_length: usize,
    pub compressed_length: usize,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ListQuery {
    #[serde(default = "default_20")]
    pub limit: usize,
    pub cursor: Option<String>,
    #[serde(default)]
    pub filter_tags: Option<String>,
    pub since: Option<String>,
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionResponse {
    pub id: String,
    pub project: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub consolidated: bool,
    pub memory_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DecayBody {
    #[serde(default = "default_factor")]
    pub factor: f32,
}

/// Telemetry metrics, cost metering, and embedding cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryResponse {
    pub metrics: rememhq_core::telemetry::MetricsSnapshot,
    pub cost_meter: rememhq_core::providers::CostSummary,
    pub cache_stats: rememhq_core::providers::CacheStats,
}

pub fn default_8() -> usize {
    8
}
pub fn default_20() -> usize {
    20
}
pub fn default_delete() -> String {
    "delete".into()
}
pub fn default_factor() -> f32 {
    0.9
}
