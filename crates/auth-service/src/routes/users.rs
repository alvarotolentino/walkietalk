use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::db;
use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;

use crate::models::{CreateDeviceRequest, DeviceResponse, UserResponse};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /users/me
// ---------------------------------------------------------------------------

pub async fn get_me(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UserResponse>, AppError> {
    let user = db::get_user(&mut state.redis.clone(), auth.user_id.0)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    Ok(Json(UserResponse::from(user)))
}

// ---------------------------------------------------------------------------
// POST /users/me/devices
// ---------------------------------------------------------------------------

pub async fn create_device(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<DeviceResponse>), AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let device = db::create_device(
        &mut state.redis.clone(),
        auth.user_id.0,
        &req.name,
        &req.platform,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(DeviceResponse::from(device))))
}

// ---------------------------------------------------------------------------
// GET /users/me/devices
// ---------------------------------------------------------------------------

pub async fn list_devices(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<DeviceResponse>>, AppError> {
    let devices = db::list_devices(&mut state.redis.clone(), auth.user_id.0).await?;

    Ok(Json(devices.into_iter().map(DeviceResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// DELETE /users/me/devices/:id
// ---------------------------------------------------------------------------

pub async fn delete_device(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = db::delete_device(&mut state.redis.clone(), device_id, auth.user_id.0).await?;

    if !deleted {
        return Err(AppError::NotFound("device not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
