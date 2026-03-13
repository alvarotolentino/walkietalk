use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rand::Rng;
use sha2::{Digest, Sha256};
use validator::Validate;

use walkietalk_shared::auth::{encode_jwt, hash_password, verify_password};
use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;
use walkietalk_shared::ids::UserId;

use crate::models::{
    AuthResponse, LoginRequest, LogoutRequest, RefreshRequest, RefreshTokenRecord,
    RegisterRequest, TokenResponse, User, UserResponse,
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

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(&req.username)
    .bind(&req.email)
    .bind(&password_hash)
    .bind(&req.display_name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            // PostgreSQL unique-violation SQLSTATE
            if db_err.code().as_deref() == Some("23505") {
                return AppError::Conflict(
                    "user with this email or username already exists".into(),
                );
            }
        }
        AppError::Internal(e.to_string())
    })?;

    let user_id = UserId(user.id);
    let access_token = encode_jwt(&user_id, None, &state.jwt_secret)?;
    let (refresh_token, token_hash) = generate_refresh_token();

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) \
         VALUES ($1, $2, NOW() + INTERVAL '7 days')",
    )
    .bind(user.id)
    .bind(&token_hash)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

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
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Unauthorized("invalid email or password".into()))?;

    if !verify_password(&req.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("invalid email or password".into()));
    }

    let user_id = UserId(user.id);
    let access_token = encode_jwt(&user_id, None, &state.jwt_secret)?;
    let (refresh_token, token_hash) = generate_refresh_token();

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) \
         VALUES ($1, $2, NOW() + INTERVAL '7 days')",
    )
    .bind(user.id)
    .bind(&token_hash)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

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

    let record = sqlx::query_as::<_, RefreshTokenRecord>(
        "SELECT id, user_id, device_id FROM refresh_tokens \
         WHERE token_hash = $1 AND revoked = false AND expires_at > NOW()",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .ok_or_else(|| AppError::Unauthorized("invalid or expired refresh token".into()))?;

    // Revoke the old token
    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE id = $1")
        .bind(record.id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Issue a new pair
    let user_id = UserId(record.user_id);
    let device_id = record.device_id.map(walkietalk_shared::ids::DeviceId);
    let access_token = encode_jwt(&user_id, device_id.as_ref(), &state.jwt_secret)?;
    let (new_refresh_token, new_token_hash) = generate_refresh_token();

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, device_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, NOW() + INTERVAL '7 days')",
    )
    .bind(record.user_id)
    .bind(record.device_id)
    .bind(&new_token_hash)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

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
            sqlx::query(
                "UPDATE refresh_tokens SET revoked = true \
                 WHERE token_hash = $1 AND user_id = $2",
            )
            .bind(&token_hash)
            .bind(auth.user_id.0)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }
        None => {
            sqlx::query(
                "UPDATE refresh_tokens SET revoked = true \
                 WHERE user_id = $1 AND revoked = false",
            )
            .bind(auth.user_id.0)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
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
