use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tauri::{AppHandle, Emitter, Manager};

use super::ws::{connect_ws, WsWriteTx};
use walkietalk_shared::audio::AudioFrame;
use walkietalk_shared::messages::{ClientMessage, ServerMessage};
use crate::state::AppState;

const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const HEARTBEAT_TIMEOUT_SECS: u64 = 90; // 3 missed heartbeats
const MAX_RECONNECT_ATTEMPTS: u32 = 10;
const BASE_DELAY_MS: u64 = 500;
const MAX_DELAY_MS: u64 = 60_000;

/// Manages the WebSocket transport: send/receive, heartbeat, event dispatch, auto-reconnect.
pub struct TransportManager {
    write_tx: WsWriteTx,
    read_task: JoinHandle<()>,
    heartbeat_task: JoinHandle<()>,
    reconnect_task: JoinHandle<()>,
    shutdown_flag: Arc<AtomicBool>,
}

impl TransportManager {
    /// Connect to the signaling server and start the read/heartbeat/reconnect loops.
    pub async fn connect(
        url: String,
        app: AppHandle,
        active_rooms: Vec<String>,
    ) -> Result<Self, String> {
        let (write_tx, read_rx) = connect_ws(&url).await?;
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let last_ack = Arc::new(AtomicU64::new(now_secs()));

        let _ = app.emit("connection_state", "connected");

        // Rejoin active rooms after connect
        rejoin_rooms(&write_tx, &active_rooms).await;

        // Notify channel: read_task signals when connection drops
        let (drop_tx, drop_rx) = tokio::sync::oneshot::channel::<()>();

        // Read loop
        let read_task = {
            let app = app.clone();
            let last_ack = last_ack.clone();
            let shutdown_flag = shutdown_flag.clone();
            let mut read_rx = read_rx;
            tokio::spawn(async move {
                Self::read_loop(&mut read_rx, &app, &last_ack).await;
                // Stop audio pipelines when connection drops
                Self::stop_audio(&app).await;
                if !shutdown_flag.load(Ordering::Relaxed) {
                    let _ = app.emit("connection_state", serde_json::json!({
                        "state": "disconnected",
                        "will_reconnect": true,
                    }).to_string());
                }
                let _ = drop_tx.send(());
            })
        };

        // Heartbeat loop with timeout detection
        let heartbeat_task = {
            let write_tx = write_tx.clone();
            let last_ack = last_ack.clone();
            let shutdown_flag = shutdown_flag.clone();
            let app = app.clone();
            tokio::spawn(async move {
                Self::heartbeat_loop(&write_tx, &last_ack, &shutdown_flag, &app).await;
            })
        };

        // Reconnect task: waits for initial connection to drop, then retries with backoff
        let reconnect_task = {
            let url = url.clone();
            let app = app.clone();
            let shutdown_flag = shutdown_flag.clone();
            let active_rooms = active_rooms.clone();
            tokio::spawn(async move {
                // Wait for the first connection to drop
                let _ = drop_rx.await;
                Self::reconnect_loop(url, app, shutdown_flag, active_rooms).await;
            })
        };

        Ok(Self {
            write_tx,
            read_task,
            heartbeat_task,
            reconnect_task,
            shutdown_flag,
        })
    }

    /// Send a text (JSON control) message over the WebSocket.
    pub async fn send_text(&self, text: &str) -> Result<(), String> {
        self.write_tx
            .send(WsMessage::Text(text.into()))
            .await
            .map_err(|_| "Transport closed".to_string())
    }

    /// Send a binary (audio) frame over the WebSocket.
    pub async fn send_binary(&self, data: Vec<u8>) -> Result<(), String> {
        self.write_tx
            .send(WsMessage::Binary(data.into()))
            .await
            .map_err(|_| "Transport closed".to_string())
    }

    /// Clone the write channel for use by the audio capture pipeline.
    pub fn write_channel(&self) -> WsWriteTx {
        self.write_tx.clone()
    }

    /// Gracefully shut down the transport — stops reconnect loop.
    pub async fn shutdown(self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        self.heartbeat_task.abort();
        self.read_task.abort();
        self.reconnect_task.abort();
        drop(self.write_tx);
    }

    /// Read loop: dispatch incoming WebSocket messages as Tauri events.
    async fn read_loop(
        read_rx: &mut mpsc::Receiver<WsMessage>,
        app: &AppHandle,
        last_ack: &AtomicU64,
    ) {
        while let Some(msg) = read_rx.recv().await {
            match msg {
                WsMessage::Text(text) => {
                    Self::dispatch_text(&text, app, last_ack);
                }
                WsMessage::Binary(data) => {
                    // Decode AudioFrame and push to playback jitter buffer
                    if let Ok(frame) = AudioFrame::decode(&data) {
                        let state = app.state::<AppState>();
                        if let Ok(guard) = state.playback.try_lock() {
                            if let Some(ref handle) = *guard {
                                if let Err(e) = handle.push_frame(&frame) {
                                    tracing::warn!("Playback push error: {e}");
                                }
                            }
                        };
                    }
                }
                WsMessage::Close(_) => {
                    tracing::info!("WebSocket closed by server");
                    break;
                }
                WsMessage::Ping(_) | WsMessage::Pong(_) => {}
                _ => {}
            }
        }
    }

    /// Heartbeat loop: send heartbeat every 30s, detect timeout (no ACK in 90s).
    async fn heartbeat_loop(
        write_tx: &WsWriteTx,
        last_ack: &AtomicU64,
        shutdown_flag: &AtomicBool,
        app: &AppHandle,
    ) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
        loop {
            interval.tick().await;
            if shutdown_flag.load(Ordering::Relaxed) {
                break;
            }

            // Check if we've heard from the server recently
            let last = last_ack.load(Ordering::Relaxed);
            if now_secs() - last > HEARTBEAT_TIMEOUT_SECS {
                tracing::warn!("Heartbeat timeout — no ACK in {HEARTBEAT_TIMEOUT_SECS}s");
                let _ = app.emit("connection_state", serde_json::json!({
                    "state": "disconnected",
                    "will_reconnect": true,
                }).to_string());
                break; // Will trigger reconnect via write_tx drop
            }

            let ts = chrono::Utc::now().timestamp();
            let msg = ClientMessage::Heartbeat { ts };
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(_) => continue,
            };
            if write_tx.send(WsMessage::Text(json.into())).await.is_err() {
                break;
            }
        }
    }

    /// Reconnect loop with exponential backoff + jitter per spec §9.9.
    /// Called after the initial connection drops.
    async fn reconnect_loop(
        url: String,
        app: AppHandle,
        shutdown_flag: Arc<AtomicBool>,
        active_rooms: Vec<String>,
    ) {
        let mut attempt: u32 = 0;

        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                return;
            }
            if attempt >= MAX_RECONNECT_ATTEMPTS {
                tracing::error!("Max reconnect attempts ({MAX_RECONNECT_ATTEMPTS}) reached");
                let _ = app.emit("connection_state", serde_json::json!({
                    "state": "failed",
                    "will_reconnect": false,
                }).to_string());
                return;
            }

            let delay = backoff_delay(attempt);
            tracing::info!("Reconnect attempt {}/{MAX_RECONNECT_ATTEMPTS} in {delay}ms", attempt + 1);
            let _ = app.emit("reconnecting", serde_json::json!({ "attempt": attempt + 1 }).to_string());

            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

            if shutdown_flag.load(Ordering::Relaxed) {
                return;
            }

            match connect_ws(&url).await {
                Ok((write_tx, mut read_rx)) => {
                    tracing::info!("Reconnected successfully");
                    let _ = app.emit("connection_state", "connected");
                    attempt = 0;

                    rejoin_rooms(&write_tx, &active_rooms).await;

                    let last_ack = Arc::new(AtomicU64::new(now_secs()));
                    let hb = {
                        let app = app.clone();
                        let last_ack = last_ack.clone();
                        let shutdown_flag = shutdown_flag.clone();
                        let write_tx = write_tx.clone();
                        tokio::spawn(async move {
                            Self::heartbeat_loop(&write_tx, &last_ack, &shutdown_flag, &app).await;
                        })
                    };

                    // Blocks until this reconnected session drops
                    Self::read_loop(&mut read_rx, &app, &last_ack).await;
                    hb.abort();
                    // Stop audio pipelines when connection drops
                    Self::stop_audio(&app).await;

                    if shutdown_flag.load(Ordering::Relaxed) {
                        return;
                    }

                    tracing::warn!("Connection lost after reconnect, retrying...");
                    let _ = app.emit("connection_state", serde_json::json!({
                        "state": "disconnected",
                        "will_reconnect": true,
                    }).to_string());
                    // Reset attempt counter since we had a successful connection
                }
                Err(e) => {
                    tracing::warn!("Reconnect attempt {} failed: {e}", attempt + 1);
                    attempt += 1;
                }
            }
        }
    }

    /// Stop audio capture and playback when the WS connection drops.
    async fn stop_audio(app: &AppHandle) {
        let state = app.state::<AppState>();
        {
            let mut cap = state.capture.lock().await;
            if let Some(handle) = cap.take() {
                handle.stop();
            }
        }
        {
            let mut pb = state.playback.lock().await;
            if let Some(handle) = pb.take() {
                handle.stop();
            }
        }
    }

    /// Parse a server JSON message and emit the corresponding Tauri event.
    /// Emits the parsed serde_json::Value so the frontend receives a proper JS object.
    fn dispatch_text(text: &str, app: &AppHandle, last_ack: &AtomicU64) {
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {e}");
                return;
            }
        };

        // Also parse as ServerMessage for variant matching
        let msg: ServerMessage = match serde_json::from_value(value.clone()) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Unknown server message: {e}");
                return;
            }
        };

        match &msg {
            ServerMessage::RoomState { .. } => {
                let _ = app.emit("room_state", &value);
            }
            ServerMessage::FloorGranted { .. } => {
                let _ = app.emit("floor_granted", &value);
            }
            ServerMessage::FloorDenied { .. } => {
                let _ = app.emit("floor_denied", &value);
            }
            ServerMessage::FloorOccupied { .. } => {
                let _ = app.emit("floor_occupied", &value);
            }
            ServerMessage::FloorReleased { .. } => {
                let _ = app.emit("floor_released", &value);
            }
            ServerMessage::FloorTimeout { .. } => {
                let _ = app.emit("floor_timeout", &value);
            }
            ServerMessage::PresenceUpdate { .. } => {
                let _ = app.emit("presence_update", &value);
            }
            ServerMessage::MemberJoined { .. } => {
                let _ = app.emit("member_joined", &value);
            }
            ServerMessage::MemberLeft { .. } => {
                let _ = app.emit("member_left", &value);
            }
            ServerMessage::HeartbeatAck { .. } => {
                last_ack.store(now_secs(), Ordering::Relaxed);
            }
            ServerMessage::Error { code, message } => {
                tracing::warn!("Server error {code}: {message}");
                let _ = app.emit("server_error", &value);
            }
        }
    }
}

/// Send JoinRoom for each active room on reconnect.
async fn rejoin_rooms(write_tx: &WsWriteTx, room_ids: &[String]) {
    for room_id in room_ids {
        let room_uuid = match uuid::Uuid::parse_str(room_id) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let msg = ClientMessage::JoinRoom {
            room_id: walkietalk_shared::ids::RoomId(room_uuid),
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = write_tx.send(WsMessage::Text(json.into())).await;
        }
    }
}

/// Exponential backoff with full jitter: min(60s, 500ms * 2^attempt) + random(0..delay/2)
fn backoff_delay(attempt: u32) -> u64 {
    use rand::Rng;
    let base = BASE_DELAY_MS.saturating_mul(1u64 << attempt.min(20));
    let capped = base.min(MAX_DELAY_MS);
    let jitter = rand::thread_rng().gen_range(0..=capped / 2);
    capped + jitter
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
