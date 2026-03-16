use std::sync::Arc;

use dashmap::DashMap;
use sqlx::PgPool;
use walkietalk_shared::extractors::HasJwtSecret;
use walkietalk_shared::ids::RoomId;

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
    /// Maps wire room_id (lock_key as i64) → RoomId (UUID).
    pub lock_key_map: Arc<DashMap<i64, RoomId>>,
}

impl HasJwtSecret for AppState {
    fn jwt_secret(&self) -> &str {
        &self.jwt_secret
    }
}
