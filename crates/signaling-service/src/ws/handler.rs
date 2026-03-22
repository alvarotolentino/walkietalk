use std::sync::Arc;

use axum::extract::ws::WebSocket;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use walkietalk_shared::auth::decode_jwt;
use walkietalk_shared::db;
use walkietalk_shared::error::AppError;
use walkietalk_shared::ids::UserId;

use crate::state::AppState;
use crate::ws::connection::handle_connection;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    token: String,
}

/// GET /ws?token=<jwt> — upgrades to WebSocket after JWT validation.
pub async fn ws_upgrade(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    // Validate JWT
    let claims = decode_jwt(&params.token, &state.jwt_secret)?;
    let sub_uuid: Uuid = claims
        .sub
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid token subject".into()))?;
    let user_id = UserId(sub_uuid);

    // Look up display_name for this user
    let display_name = db::get_display_name(&mut state.redis.clone(), sub_uuid)
        .await?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    Ok(ws.on_upgrade(move |socket: WebSocket| {
        handle_connection(socket, user_id, display_name, state)
    }))
}
