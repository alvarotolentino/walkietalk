use tokio::sync::{Mutex, RwLock};

use crate::transport::manager::TransportManager;

/// User info cached after login.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
}

/// Tokens stored after auth.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

/// Server URL + auth + transport state shared across all Tauri commands.
pub struct AppState {
    pub server_url: RwLock<String>,
    pub tokens: RwLock<Option<TokenPair>>,
    pub user: RwLock<Option<UserInfo>>,
    pub transport: Mutex<Option<TransportManager>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            server_url: RwLock::new("http://localhost:3000".to_string()),
            tokens: RwLock::new(None),
            user: RwLock::new(None),
            transport: Mutex::new(None),
        }
    }

    /// Get the current access token, if any.
    pub async fn access_token(&self) -> Option<String> {
        self.tokens.read().await.as_ref().map(|t| t.access_token.clone())
    }

    /// Get the current server URL.
    pub async fn base_url(&self) -> String {
        self.server_url.read().await.clone()
    }
}
