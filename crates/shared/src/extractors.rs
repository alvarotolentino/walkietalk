use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::auth::decode_jwt;
use crate::error::AppError;
use crate::ids::{DeviceId, UserId};

/// Trait that state types must implement to provide the JWT secret for token validation.
/// A blanket impl is provided for `Arc<T>` so both `AppState` and `Arc<AppState>` work.
pub trait HasJwtSecret {
    fn jwt_secret(&self) -> &str;
}

impl<T: HasJwtSecret> HasJwtSecret for std::sync::Arc<T> {
    fn jwt_secret(&self) -> &str {
        (**self).jwt_secret()
    }
}

/// Axum extractor that validates the `Authorization: Bearer <JWT>` header
/// and provides the authenticated user's identity.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: UserId,
    pub device_id: Option<DeviceId>,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: HasJwtSecret + Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("invalid authorization header format".into()))?;

        let claims = decode_jwt(token, state.jwt_secret())?;

        let user_id = UserId(
            Uuid::parse_str(&claims.sub)
                .map_err(|_| AppError::Unauthorized("invalid token subject".into()))?,
        );

        let device_id = claims
            .device_id
            .as_deref()
            .map(|d| Uuid::parse_str(d).map(DeviceId))
            .transpose()
            .map_err(|_| AppError::Unauthorized("invalid device id in token".into()))?;

        Ok(AuthUser { user_id, device_id })
    }
}
