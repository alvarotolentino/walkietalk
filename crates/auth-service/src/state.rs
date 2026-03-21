use walkietalk_shared::db::RedisConn;
use walkietalk_shared::extractors::HasJwtSecret;

/// Shared application state for all auth service handlers.
#[derive(Clone)]
pub struct AppState {
    pub redis: RedisConn,
    pub jwt_secret: String,
}

impl HasJwtSecret for AppState {
    fn jwt_secret(&self) -> &str {
        &self.jwt_secret
    }
}
