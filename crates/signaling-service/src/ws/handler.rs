use std::sync::Arc;

use axum::extract::ws::WebSocket;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use walkietalk_shared::auth::decode_jwt;
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
    let display_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(sub_uuid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

    Ok(ws.on_upgrade(move |socket: WebSocket| {
        handle_connection(socket, user_id, display_name, state)
    }))
}
