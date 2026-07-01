use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use rememhq_core::memory::types::{
    MemoryRecord, MemoryStoreRecord, MemoryType, MemoryVersionRecord,
};
use rememhq_core::reasoning::ReasoningEngine;

use crate::middleware::auth::check_auth;
use crate::routes::memories::ErrorResponse;

type AppState = Arc<ReasoningEngine>;

#[derive(Deserialize)]
pub struct CreateStoreRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateStoreMemoryRequest {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct UpdateStoreMemoryRequest {
    pub content: String,
}

pub async fn create_store(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateStoreRequest>,
) -> Result<(StatusCode, Json<MemoryStoreRecord>), (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let store = engine
        .create_store(&req.name, req.description.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok((StatusCode::CREATED, Json(store)))
}

pub async fn list_stores(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MemoryStoreRecord>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let stores = engine.list_stores().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(stores))
}

pub async fn get_store(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(store_id): Path<String>,
) -> Result<Json<MemoryStoreRecord>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let store = engine.get_store(&store_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    if let Some(store) = store {
        Ok(Json(store))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Store not found".into(),
            }),
        ))
    }
}

pub async fn archive_store(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(store_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let success = engine.archive_store(&store_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    if success {
        Ok(StatusCode::OK)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Store not found".into(),
            }),
        ))
    }
}

pub async fn list_store_memories(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(store_id): Path<String>,
) -> Result<Json<Vec<MemoryRecord>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let memories = engine
        .list_memories_by_store(&store_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(memories))
}

pub async fn create_store_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(store_id): Path<String>,
    Json(req): Json<CreateStoreMemoryRequest>,
) -> Result<(StatusCode, Json<MemoryRecord>), (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    // Check if memory by path already exists
    if let Ok(Some(_)) = engine.get_memory_by_path(&store_id, &req.path).await {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Memory with this path already exists in the store".into(),
            }),
        ));
    }

    let mut record = MemoryRecord::new(&req.content, MemoryType::Fact);
    record.store_id = Some(store_id);
    record.path = Some(req.path);

    let options = crate::middleware::auth::extract_provider_options(&headers);
    let stored = engine.store_memory(record, false, options.as_ref()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok((StatusCode::CREATED, Json(stored)))
}

pub async fn get_store_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path((store_id, path_or_id)): Path<(String, String)>,
) -> Result<Json<MemoryRecord>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let memory = if let Ok(_id) = Uuid::parse_str(&path_or_id) {
        // Since we don't have get_memory_by_id_and_store exposed easily, let's just use path.
        engine
            .get_memory_by_path(&store_id, &path_or_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?
    } else {
        engine
            .get_memory_by_path(&store_id, &path_or_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?
    };

    if let Some(mem) = memory {
        Ok(Json(mem))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Memory not found".into(),
            }),
        ))
    }
}

pub async fn update_store_memory(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path((store_id, path)): Path<(String, String)>,
    Json(req): Json<UpdateStoreMemoryRequest>,
) -> Result<Json<MemoryRecord>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let memory = engine
        .get_memory_by_path(&store_id, &path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    if let Some(mem) = memory {
        let options = crate::middleware::auth::extract_provider_options(&headers);
        let updated = engine
            .update_memory(mem.id, Some(req.content), None, None, options.as_ref())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?;
        Ok(Json(updated))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Memory not found".into(),
            }),
        ))
    }
}

pub async fn list_memory_versions(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path((store_id, path)): Path<(String, String)>,
) -> Result<Json<Vec<MemoryVersionRecord>>, (StatusCode, Json<ErrorResponse>)> {
    check_auth(&headers)?;

    let memory = engine
        .get_memory_by_path(&store_id, &path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    if let Some(mem) = memory {
        let versions = engine
            .list_memory_versions(&store_id, mem.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?;
        Ok(Json(versions))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Memory not found".into(),
            }),
        ))
    }
}
