use serde::{Deserialize, Serialize};
use tauri::State;

use crate::http_client::HttpClient;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub owner_id: String,
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSettings {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub member_count: i64,
    pub invite_code: Option<String>,
    pub members: Vec<RoomMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMember {
    pub user_id: String,
    pub display_name: String,
    pub role: String,
}

#[tauri::command]
pub async fn get_rooms(state: State<'_, AppState>) -> Result<Vec<Room>, String> {
    let http = HttpClient::new();
    let req = http.sig_get(&state, "/rooms").await?;
    http.send_json(req).await
}

#[tauri::command]
pub async fn create_room(
    name: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<Room, String> {
    let http = HttpClient::new();
    let req = http
        .sig_post(&state, "/rooms")
        .await?
        .json(&serde_json::json!({
            "name": name,
            "description": description,
        }));
    http.send_json(req).await
}

#[tauri::command]
pub async fn join_by_code(code: String, state: State<'_, AppState>) -> Result<Room, String> {
    let http = HttpClient::new();
    let req = http
        .sig_post(&state, "/rooms/join")
        .await?
        .json(&serde_json::json!({ "invite_code": code }));
    http.send_json(req).await
}

#[tauri::command]
pub async fn leave_room(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let http = HttpClient::new();
    let path = format!("/rooms/{room_id}/leave");
    let req = http.sig_post(&state, &path).await?;
    http.send_empty(req).await
}

#[tauri::command]
pub async fn get_room_settings(
    room_id: String,
    state: State<'_, AppState>,
) -> Result<RoomSettings, String> {
    let http = HttpClient::new();
    let path = format!("/rooms/{room_id}");
    let req = http.sig_get(&state, &path).await?;
    http.send_json(req).await
}

#[tauri::command]
pub async fn update_room(
    room_id: String,
    name: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let http = HttpClient::new();
    let path = format!("/rooms/{room_id}");
    let req = http.sig_put(&state, &path).await?.json(&serde_json::json!({
        "name": name,
        "description": description,
    }));
    http.send_empty(req).await
}

#[tauri::command]
pub async fn delete_room(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let http = HttpClient::new();
    let path = format!("/rooms/{room_id}");
    let req = http.sig_delete(&state, &path).await?;
    http.send_empty(req).await
}

#[tauri::command]
pub async fn regenerate_invite(
    room_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let http = HttpClient::new();
    let path = format!("/rooms/{room_id}/invite");
    let req = http.sig_post(&state, &path).await?;

    #[derive(Deserialize)]
    struct InviteResp {
        invite_code: String,
    }
    let resp: InviteResp = http.send_json(req).await?;
    Ok(resp.invite_code)
}
