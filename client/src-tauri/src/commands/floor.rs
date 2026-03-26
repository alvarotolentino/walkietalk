use tauri::State;

use crate::state::AppState;
use walkietalk_shared::ids::RoomId;
use walkietalk_shared::messages::ClientMessage;

#[tauri::command]
pub async fn request_floor(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let room_uuid = uuid::Uuid::parse_str(&room_id).map_err(|_| "Invalid room ID".to_string())?;

    let transport = state.transport.lock().await;
    let t = transport.as_ref().ok_or("Not connected")?;

    let msg = ClientMessage::FloorRequest {
        room_id: RoomId(room_uuid),
    };
    t.send_text(&serde_json::to_string(&msg).map_err(|e| e.to_string())?)
        .await
}

#[tauri::command]
pub async fn release_floor(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let room_uuid = uuid::Uuid::parse_str(&room_id).map_err(|_| "Invalid room ID".to_string())?;

    let transport = state.transport.lock().await;
    let t = transport.as_ref().ok_or("Not connected")?;

    let msg = ClientMessage::FloorRelease {
        room_id: RoomId(room_uuid),
    };
    t.send_text(&serde_json::to_string(&msg).map_err(|e| e.to_string())?)
        .await
}
