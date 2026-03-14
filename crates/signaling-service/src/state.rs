use std::sync::Arc;

use sqlx::PgPool;
use walkietalk_shared::extractors::HasJwtSecret;

use crate::floor::FloorManager;
use crate::hub::WsHub;
use crate::presence::PresenceManager;

/// Shared application state for all signaling service handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
    pub ws_hub: Arc<WsHub>,
    pub floor_manager: Arc<FloorManager>,
    pub presence: Arc<PresenceManager>,
}

impl HasJwtSecret for AppState {
    fn jwt_secret(&self) -> &str {
        &self.jwt_secret
    }
}
