use std::collections::HashSet;
use tokio::sync::{Mutex, RwLock};

use crate::audio::capture::CaptureHandle;
use crate::audio::playback::PlaybackHandle;
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
    pub signaling_url: RwLock<String>,
    pub tokens: RwLock<Option<TokenPair>>,
    pub user: RwLock<Option<UserInfo>>,
    pub transport: Mutex<Option<TransportManager>>,
    /// Room IDs currently joined via WebSocket (for rejoin on reconnect).
    pub active_rooms: RwLock<HashSet<String>>,
    /// Active audio capture handle (set when transmitting).
    pub capture: Mutex<Option<CaptureHandle>>,
    /// Active audio playback handle (set when receiving).
    pub playback: Mutex<Option<PlaybackHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            server_url: RwLock::new("http://localhost:3001".to_string()),
            signaling_url: RwLock::new("http://localhost:3002".to_string()),
            tokens: RwLock::new(None),
            user: RwLock::new(None),
            transport: Mutex::new(None),
            active_rooms: RwLock::new(HashSet::new()),
            capture: Mutex::new(None),
            playback: Mutex::new(None),
        }
    }

    /// Get the current access token, if any.
    pub async fn access_token(&self) -> Option<String> {
        self.tokens.read().await.as_ref().map(|t| t.access_token.clone())
    }

    /// Get the current auth server URL.
    pub async fn base_url(&self) -> String {
        self.server_url.read().await.clone()
    }

    /// Get the current signaling server URL.
    pub async fn signaling_base_url(&self) -> String {
        self.signaling_url.read().await.clone()
    }

    /// Graceful shutdown: stop audio devices, close WebSocket transport,
    /// and clear active rooms — called when the app is about to exit.
    pub async fn graceful_shutdown(&self) {
        // 1. Stop audio capture (mic release)
        if let Some(handle) = self.capture.lock().await.take() {
            handle.stop();
            tracing::info!("Audio capture stopped");
        }

        // 2. Stop audio playback (speaker release + decoder reset)
        if let Some(handle) = self.playback.lock().await.take() {
            handle.stop();
            tracing::info!("Audio playback stopped");
        }

        // 3. Shut down WebSocket transport (heartbeat, read loop, reconnect)
        if let Some(transport) = self.transport.lock().await.take() {
            transport.shutdown().await;
            tracing::info!("WebSocket transport shut down");
        }

        // 4. Clear active rooms so a future reconnect starts clean
        self.active_rooms.write().await.clear();

        tracing::info!("Graceful shutdown complete");
    }
}
