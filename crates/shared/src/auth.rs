use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::ids::{DeviceId, UserId};

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the user ID as a UUID string.
    pub sub: String,
    /// Optional device ID that scoped this token.
    pub device_id: Option<String>,
    /// Expiration time (seconds since epoch).
    pub exp: usize,
    /// Issued-at time (seconds since epoch).
    pub iat: usize,
}

/// Encode a JWT access token with a 15-minute expiry.
pub fn encode_jwt(
    user_id: &UserId,
    device_id: Option<&DeviceId>,
    secret: &str,
) -> Result<String, AppError> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.0.to_string(),
        device_id: device_id.map(|d| d.0.to_string()),
        exp: now + 15 * 60, // 15 minutes
        iat: now,
    };
    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("jwt encode error: {e}")))
}

/// Decode and validate a JWT access token.
pub fn decode_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let validation = Validation::default(); // HS256, validates exp
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
            AppError::Unauthorized("token expired".into())
        }
        _ => AppError::Unauthorized(format!("invalid token: {e}")),
    })
}

/// Hash a password with Argon2id (time_cost=2, memory_cost=64 MiB, parallelism=1).
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(65536, 2, 1, None)
        .map_err(|e| AppError::Internal(format!("argon2 params error: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("password hash error: {e}")))?;
    Ok(hash.to_string())
}

/// Verify a password against an Argon2id hash string.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("password hash parse error: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip() {
        let user_id = UserId(uuid::Uuid::new_v4());
        let secret = "test-secret";
        let token = encode_jwt(&user_id, None, secret).expect("encode");
        let claims = decode_jwt(&token, secret).expect("decode");
        assert_eq!(claims.sub, user_id.0.to_string());
        assert!(claims.device_id.is_none());
    }

    #[test]
    fn jwt_with_device_id() {
        let user_id = UserId(uuid::Uuid::new_v4());
        let device_id = DeviceId(uuid::Uuid::new_v4());
        let secret = "test-secret";
        let token = encode_jwt(&user_id, Some(&device_id), secret).expect("encode");
        let claims = decode_jwt(&token, secret).expect("decode");
        assert_eq!(claims.device_id.unwrap(), device_id.0.to_string());
    }

    #[test]
    fn jwt_wrong_secret_fails() {
        let user_id = UserId(uuid::Uuid::new_v4());
        let token = encode_jwt(&user_id, None, "secret-a").expect("encode");
        let result = decode_jwt(&token, "secret-b");
        assert!(result.is_err());
    }

    #[test]
    fn password_hash_and_verify() {
        let password = "hunter2hunter2";
        let hash = hash_password(password).expect("hash");
        assert!(verify_password(password, &hash).expect("verify"));
        assert!(!verify_password("wrong-password", &hash).expect("verify"));
    }
}
