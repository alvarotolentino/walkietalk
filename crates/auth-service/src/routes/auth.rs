use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rand::Rng;
use sha2::{Digest, Sha256};
use validator::Validate;

use walkietalk_shared::auth::{encode_jwt, hash_password, verify_password};
use walkietalk_shared::db;
use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;
use walkietalk_shared::ids::UserId;

use crate::models::{
    AuthResponse, LoginRequest, LogoutRequest, RefreshRequest,
    RegisterRequest, TokenResponse, UserResponse,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /auth/register
// ---------------------------------------------------------------------------

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let password_hash = hash_password(&req.password)?;

    let user = db::create_user(
        &mut state.redis.clone(),
        &req.username,
        &req.email,
        &password_hash,
        &req.display_name,
    )
    .await?;

    let user_id = UserId(user.id);
    let access_token = encode_jwt(&user_id, None, &state.jwt_secret)?;
    let (refresh_token, token_hash) = generate_refresh_token();

    db::create_refresh_token(&mut state.redis.clone(), user.id, None, &token_hash).await?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token,
            refresh_token,
            user: UserResponse::from(user),
        }),
    ))
}

// ---------------------------------------------------------------------------
// POST /auth/login
// ---------------------------------------------------------------------------

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let user = db::get_user_by_email(&mut state.redis.clone(), &req.email)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid email or password".into()))?;

    if !verify_password(&req.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("invalid email or password".into()));
    }

    let user_id = UserId(user.id);
    let access_token = encode_jwt(&user_id, None, &state.jwt_secret)?;
    let (refresh_token, token_hash) = generate_refresh_token();

    db::create_refresh_token(&mut state.redis.clone(), user.id, None, &token_hash).await?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        user: UserResponse::from(user),
    }))
}

// ---------------------------------------------------------------------------
// POST /auth/refresh
// ---------------------------------------------------------------------------

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let token_hash = hex::encode(Sha256::digest(req.refresh_token.as_bytes()));

    let record = db::get_refresh_token(&mut state.redis.clone(), &token_hash)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid or expired refresh token".into()))?;

    // Revoke the old token
    db::revoke_refresh_token(&mut state.redis.clone(), &token_hash).await?;

    // Issue a new pair
    let user_id = UserId(record.user_id);
    let device_id = record.device_id.map(walkietalk_shared::ids::DeviceId);
    let access_token = encode_jwt(&user_id, device_id.as_ref(), &state.jwt_secret)?;
    let (new_refresh_token, new_token_hash) = generate_refresh_token();

    db::create_refresh_token(
        &mut state.redis.clone(),
        record.user_id,
        record.device_id,
        &new_token_hash,
    )
    .await?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: new_refresh_token,
    }))
}

// ---------------------------------------------------------------------------
// POST /auth/logout
// ---------------------------------------------------------------------------

pub async fn logout(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<LogoutRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    match req.refresh_token {
        Some(token) => {
            let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
            db::revoke_refresh_token(&mut state.redis.clone(), &token_hash).await?;
        }
        None => {
            db::revoke_all_refresh_tokens(&mut state.redis.clone(), auth.user_id.0).await?;
        }
    }

    Ok(Json(serde_json::json!({ "message": "logged out" })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a cryptographically-random refresh token and its SHA-256 hash.
/// Returns `(raw_token_hex, hash_hex)`.
fn generate_refresh_token() -> (String, String) {
    let raw: [u8; 32] = rand::thread_rng().gen();
    let token = hex::encode(raw);
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    (token, hash)
}
