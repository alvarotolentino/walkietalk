use tauri::{AppHandle, State};

use crate::state::AppState;
use crate::transport::manager::TransportManager;

#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let token = state
        .access_token()
        .await
        .ok_or_else(|| "Not authenticated".to_string())?;

    let base = state.base_url().await;

    // Build WebSocket URL from the HTTP base
    let ws_url = base
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    let url = format!("{ws_url}/ws?token={token}");

    let mut transport_lock = state.transport.lock().await;
    // Shutdown existing connection if any
    if let Some(t) = transport_lock.take() {
        t.shutdown().await;
    }

    let manager = TransportManager::connect(url, app.clone()).await?;
    *transport_lock = Some(manager);

    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let mut transport = state.transport.lock().await;
    if let Some(t) = transport.take() {
        t.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn reconnect(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Re-use the connect command
    connect(app, state).await
}
