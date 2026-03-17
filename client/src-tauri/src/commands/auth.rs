use serde::Deserialize;
use tauri::State;

use crate::http_client::HttpClient;
use crate::state::{AppState, TokenPair, UserInfo};

#[derive(Deserialize)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    user: AuthUser,
}

#[derive(Deserialize)]
struct AuthUser {
    id: String,
    username: String,
    email: String,
    display_name: String,
}

#[tauri::command]
pub async fn login(
    email: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<UserInfo, String> {
    let http = HttpClient::new();
    let req = http
        .post(&state, "/auth/login")
        .await?
        .json(&serde_json::json!({ "email": email, "password": password }));
    let resp: AuthResponse = http.send_json(req).await.map_err(|e| {
        if e.contains("401") || e.contains("Session expired") {
            "invalid_credentials".to_string()
        } else {
            e
        }
    })?;

    let user_info = UserInfo {
        id: resp.user.id,
        username: resp.user.username,
        email: resp.user.email,
        display_name: resp.user.display_name,
    };

    *state.tokens.write().await = Some(TokenPair {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
    });
    *state.user.write().await = Some(user_info.clone());

    Ok(user_info)
}

#[tauri::command]
pub async fn register(
    display_name: String,
    username: String,
    email: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<UserInfo, String> {
    let http = HttpClient::new();
    let req = http
        .post(&state, "/auth/register")
        .await?
        .json(&serde_json::json!({
            "display_name": display_name,
            "username": username,
            "email": email,
            "password": password,
        }));
    let resp: AuthResponse = http.send_json(req).await?;

    let user_info = UserInfo {
        id: resp.user.id,
        username: resp.user.username,
        email: resp.user.email,
        display_name: resp.user.display_name,
    };

    *state.tokens.write().await = Some(TokenPair {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
    });
    *state.user.write().await = Some(user_info.clone());

    Ok(user_info)
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    // Attempt to call server logout endpoint (best-effort)
    if let Some(tokens) = state.tokens.read().await.as_ref() {
        let http = HttpClient::new();
        let req = http.post(&state, "/auth/logout").await;
        if let Ok(req) = req {
            let _ = http
                .send_empty(req.json(&serde_json::json!({
                    "refresh_token": tokens.refresh_token,
                })))
                .await;
        }
    }

    *state.tokens.write().await = None;
    *state.user.write().await = None;

    // Disconnect transport if active
    let mut transport = state.transport.lock().await;
    if let Some(t) = transport.take() {
        t.shutdown().await;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_current_user(state: State<'_, AppState>) -> Result<UserInfo, String> {
    let user = state.user.read().await;
    user.clone().ok_or_else(|| "not_authenticated".to_string())
}
