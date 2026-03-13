use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;

use crate::models::{
    CreateDeviceRequest, Device, DeviceResponse, User, UserResponse,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /users/me
// ---------------------------------------------------------------------------

pub async fn get_me(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UserResponse>, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id.0)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
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

    let device = sqlx::query_as::<_, Device>(
        "INSERT INTO devices (user_id, name, platform) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(auth.user_id.0)
    .bind(&req.name)
    .bind(&req.platform)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(DeviceResponse::from(device))))
}

// ---------------------------------------------------------------------------
// GET /users/me/devices
// ---------------------------------------------------------------------------

pub async fn list_devices(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<DeviceResponse>>, AppError> {
    let devices = sqlx::query_as::<_, Device>(
        "SELECT * FROM devices WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id.0)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

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
    let result = sqlx::query("DELETE FROM devices WHERE id = $1 AND user_id = $2")
        .bind(device_id)
        .bind(auth.user_id.0)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("device not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
