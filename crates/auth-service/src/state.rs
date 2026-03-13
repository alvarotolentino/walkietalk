use sqlx::PgPool;
use walkietalk_shared::extractors::HasJwtSecret;

/// Shared application state for all auth service handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
}

impl HasJwtSecret for AppState {
    fn jwt_secret(&self) -> &str {
        &self.jwt_secret
    }
}
