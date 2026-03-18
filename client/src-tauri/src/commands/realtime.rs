use tauri::State;

use crate::state::AppState;
use walkietalk_shared::messages::ClientMessage;
use walkietalk_shared::ids::RoomId;

#[tauri::command]
pub async fn join_room_ws(
    room_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let room_uuid = uuid::Uuid::parse_str(&room_id)
        .map_err(|_| "Invalid room ID".to_string())?;

    let transport = state.transport.lock().await;
    let t = transport.as_ref().ok_or("Not connected")?;

    let msg = ClientMessage::JoinRoom {
        room_id: RoomId(room_uuid),
    };
    t.send_text(&serde_json::to_string(&msg).map_err(|e| e.to_string())?)
        .await?;

    // Track active room for rejoin on reconnect
    state.active_rooms.write().await.insert(room_id);
    Ok(())
}

#[tauri::command]
pub async fn leave_room_ws(
    room_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let room_uuid = uuid::Uuid::parse_str(&room_id)
        .map_err(|_| "Invalid room ID".to_string())?;

    let transport = state.transport.lock().await;
    let t = transport.as_ref().ok_or("Not connected")?;

    let msg = ClientMessage::LeaveRoom {
        room_id: RoomId(room_uuid),
    };
    t.send_text(&serde_json::to_string(&msg).map_err(|e| e.to_string())?)
        .await?;

    // Remove from active rooms
    state.active_rooms.write().await.remove(&room_id);
    Ok(())
}
