//! REST endpoints for local model management (`GET /v1/models`, `POST /v1/models/pull`).
//!
//! Downloads can be multi-gigabyte (the phi-3-mini GGUF is ~2.4 GB), far
//! longer than the API's blanket 30s request timeout (see `router.rs`). So
//! `POST /v1/models/pull` does not block on the download itself: it starts
//! the download in a background task and returns immediately with
//! `status: "downloading"`. Poll `GET /v1/models` to see when the model's
//! `status` flips to `"installed"`.
//!
//! This intentionally mirrors `remem models pull` / `remem models list` in
//! the CLI (`rememhq-cli/src/main.rs`), which call the same
//! `rememhq_core::models` functions synchronously since a local CLI command
//! blocking on a download is normal, unlike an HTTP request.

use axum::{
    http::{HeaderMap, StatusCode},
    response::Json,
};
use validator::Validate;

use rememhq_core::models::{self, InstallStatus, ModelKind};

use crate::middleware::auth::check_auth;
use crate::models::{ErrorResponse, ModelInfo, PullModelRequest, PullModelResponse};

fn kind_str(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Embedding => "embedding",
        ModelKind::LocalLlm => "local_llm",
    }
}

fn status_str(status: InstallStatus) -> &'static str {
    match status {
        InstallStatus::NotInstalled => "not_installed",
        InstallStatus::PartiallyInstalled => "partially_installed",
        InstallStatus::Installed => "installed",
    }
}

/// List known local models (embedding + local-LLM) and their install status.
#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "Known local models and their install status", body = [ModelInfo]),
        (status = 401, description = "Unauthorized")
    ),
    security(("api_key" = []))
)]
pub async fn list_models(
    headers: HeaderMap,
) -> Result<Json<Vec<ModelInfo>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let dest = models::default_models_dir();
    let out = models::KNOWN_MODELS
        .iter()
        .map(|spec| ModelInfo {
            id: spec.id.to_string(),
            description: spec.description.to_string(),
            kind: kind_str(spec.kind).to_string(),
            approx_bytes: spec.approx_bytes,
            status: status_str(models::install_status(spec, &dest)).to_string(),
        })
        .collect();

    Ok(Json(out))
}

/// Pull (download) a known local model by id. Returns immediately — the
/// download runs in the background since it can take far longer than the
/// API's request timeout. Poll `GET /v1/models` to see when it finishes.
#[utoipa::path(
    post,
    path = "/v1/models/pull",
    request_body = PullModelRequest,
    responses(
        (status = 200, description = "Model was already fully downloaded", body = PullModelResponse),
        (status = 202, description = "Download started in the background", body = PullModelResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown model id", body = ErrorResponse)
    ),
    security(("api_key" = []))
)]
pub async fn pull_model(
    headers: HeaderMap,
    Json(req): Json<PullModelRequest>,
) -> Result<(StatusCode, Json<PullModelResponse>), (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    if let Err(e) = req.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        ));
    }

    let spec = models::find_model(&req.model).ok_or_else(|| {
        let known: Vec<&str> = models::KNOWN_MODELS.iter().map(|m| m.id).collect();
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!(
                    "Unknown model '{}'. Available models: {}",
                    req.model,
                    known.join(", ")
                ),
            }),
        )
    })?;

    let dest = models::default_models_dir();

    if models::install_status(spec, &dest) == InstallStatus::Installed {
        return Ok((
            StatusCode::OK,
            Json(PullModelResponse {
                model: spec.id.to_string(),
                status: "already_installed".to_string(),
                detail: format!("{} is already downloaded to {}", spec.id, dest.display()),
            }),
        ));
    }

    // ModelSpec's fields are all `&'static str` / Copy, so the clone here is
    // cheap and free of the borrow's lifetime — safe to move into a task
    // that outlives this request.
    let spec_owned = spec.clone();
    let approx_bytes = spec.approx_bytes;
    let model_id = spec.id.to_string();

    tokio::spawn(async move {
        match models::pull_model(&spec_owned, &dest).await {
            Ok(result) => {
                tracing::info!(
                    model = %spec_owned.id,
                    primary_downloaded = result.primary_downloaded,
                    secondary_downloaded = result.secondary_downloaded,
                    "model pull finished"
                );
            }
            Err(e) => {
                tracing::error!(model = %spec_owned.id, error = %e, "model pull failed");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(PullModelResponse {
            model: model_id,
            status: "downloading".to_string(),
            detail: format!(
                "Download started in the background (~{:.0} MB). Poll GET /v1/models to check progress.",
                approx_bytes as f64 / 1_000_000.0
            ),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_str_covers_all_variants() {
        assert_eq!(kind_str(ModelKind::Embedding), "embedding");
        assert_eq!(kind_str(ModelKind::LocalLlm), "local_llm");
    }

    #[test]
    fn test_status_str_covers_all_variants() {
        assert_eq!(status_str(InstallStatus::NotInstalled), "not_installed");
        assert_eq!(
            status_str(InstallStatus::PartiallyInstalled),
            "partially_installed"
        );
        assert_eq!(status_str(InstallStatus::Installed), "installed");
    }
}
