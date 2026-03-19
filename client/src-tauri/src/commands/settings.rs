use tauri::{AppHandle, State};
use tauri_plugin_store::StoreExt;

use crate::state::AppState;

#[tauri::command]
pub async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.base_url().await)
}

#[tauri::command]
pub async fn set_server_url(
    url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.server_url.write().await = url.clone();

    if let Ok(store) = app.store("settings.json") {
        let _ = store.set("server_url", serde_json::json!(url));
        let _ = store.save();
    }

    Ok(())
}

#[tauri::command]
pub async fn get_signaling_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.signaling_base_url().await)
}

#[tauri::command]
pub async fn set_signaling_url(
    url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.signaling_url.write().await = url.clone();

    if let Ok(store) = app.store("settings.json") {
        let _ = store.set("signaling_url", serde_json::json!(url));
        let _ = store.save();
    }

    Ok(())
}
