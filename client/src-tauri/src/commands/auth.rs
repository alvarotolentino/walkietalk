use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_store::StoreExt;

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
    app: AppHandle,
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

    let tokens = TokenPair {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
    };

    // Persist to store
    persist_auth(&app, &tokens, &user_info);

    *state.tokens.write().await = Some(tokens);
    *state.user.write().await = Some(user_info.clone());

    Ok(user_info)
}

#[tauri::command]
pub async fn register(
    display_name: String,
    username: String,
    email: String,
    password: String,
    app: AppHandle,
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

    let tokens = TokenPair {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
    };

    persist_auth(&app, &tokens, &user_info);

    *state.tokens.write().await = Some(tokens);
    *state.user.write().await = Some(user_info.clone());

    Ok(user_info)
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
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

    // Clear persistent store
    clear_auth(&app);

    // Disconnect transport if active
    let mut transport = state.transport.lock().await;
    if let Some(t) = transport.take() {
        t.shutdown().await;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_current_user(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UserInfo, String> {
    // Check in-memory state first
    if let Some(u) = state.user.read().await.clone() {
        return Ok(u);
    }

    // Try loading from persistent store
    let store = app.store("auth.json").map_err(|e| e.to_string())?;
    let tokens_val = store.get("tokens");
    let user_val = store.get("user");

    if let (Some(tv), Some(uv)) = (tokens_val, user_val) {
        if let (Ok(tokens), Ok(user)) = (
            serde_json::from_value::<TokenPair>(tv),
            serde_json::from_value::<UserInfo>(uv),
        ) {
            // Load tokens into memory so the HTTP client can use them
            *state.tokens.write().await = Some(tokens);
            *state.user.write().await = Some(user.clone());

            // Validate the token against the server
            let http = HttpClient::new();
            match http.get(&state, "/users/me").await {
                Ok(req) => match http.send_json::<UserInfo>(req).await {
                    Ok(fresh_user) => {
                        *state.user.write().await = Some(fresh_user.clone());
                        return Ok(fresh_user);
                    }
                    Err(_) => {
                        // Token is invalid/expired — clear everything
                        *state.tokens.write().await = None;
                        *state.user.write().await = None;
                        clear_auth(&app);
                    }
                },
                Err(_) => {
                    // Could not build request — clear state
                    *state.tokens.write().await = None;
                    *state.user.write().await = None;
                    clear_auth(&app);
                }
            }
        }
    }

    Err("not_authenticated".to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn persist_auth(app: &AppHandle, tokens: &TokenPair, user: &UserInfo) {
    if let Ok(store) = app.store("auth.json") {
        let _ = store.set(
            "tokens",
            serde_json::to_value(tokens).unwrap_or_default(),
        );
        let _ = store.set(
            "user",
            serde_json::to_value(user).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

fn clear_auth(app: &AppHandle) {
    if let Ok(store) = app.store("auth.json") {
        let _ = store.delete("tokens");
        let _ = store.delete("user");
        let _ = store.save();
    }
}
