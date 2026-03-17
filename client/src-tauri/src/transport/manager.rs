use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tauri::{AppHandle, Emitter};

use super::ws::{connect_ws, WsWriteTx};
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

/// Manages the WebSocket transport: send/receive, heartbeat, event dispatch.
pub struct TransportManager {
    write_tx: WsWriteTx,
    read_task: JoinHandle<()>,
    heartbeat_task: JoinHandle<()>,
}

impl TransportManager {
    /// Connect to the signaling server and start the read/heartbeat loops.
    pub async fn connect(url: String, app: AppHandle) -> Result<Self, String> {
        let (write_tx, read_rx) = connect_ws(&url).await?;

        // Emit "connected" event to the frontend
        let _ = app.emit("connection_state", "connected");

        // Spawn read task: dispatch incoming messages as Tauri events
        let read_task = {
            let app = app.clone();
            let mut read_rx = read_rx;
            tokio::spawn(async move {
                Self::read_loop(&mut read_rx, &app).await;
                // Connection closed — emit disconnected
                let _ = app.emit("connection_state", "disconnected");
            })
        };

        // Spawn heartbeat task: send heartbeat every 30s
        let heartbeat_task = {
            let write_tx = write_tx.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
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
            })
        };

        Ok(Self {
            write_tx,
            read_task,
            heartbeat_task,
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

    /// Gracefully shut down the transport.
    pub async fn shutdown(self) {
        self.heartbeat_task.abort();
        self.read_task.abort();
        // Drop write_tx — the write loop will exit
        drop(self.write_tx);
    }

    /// Read loop: dispatch incoming WebSocket messages to tauri events.
    async fn read_loop(read_rx: &mut mpsc::Receiver<WsMessage>, app: &AppHandle) {
        while let Some(msg) = read_rx.recv().await {
            match msg {
                WsMessage::Text(text) => {
                    Self::dispatch_text(&text, app);
                }
                WsMessage::Binary(data) => {
                    // Binary frames are audio data — emit as audio_frame event
                    let _ = app.emit("audio_frame", data.to_vec());
                }
                WsMessage::Close(_) => {
                    tracing::info!("WebSocket closed by server");
                    break;
                }
                WsMessage::Ping(_) | WsMessage::Pong(_) => {
                    // Handled at the protocol level
                }
                _ => {}
            }
        }
    }

    /// Parse a server JSON message and emit the corresponding Tauri event.
    fn dispatch_text(text: &str, app: &AppHandle) {
        let msg: ServerMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse server message: {e}");
                return;
            }
        };

        match &msg {
            ServerMessage::RoomState { .. } => {
                let _ = app.emit("room_state", text);
            }
            ServerMessage::FloorGranted { .. } => {
                let _ = app.emit("floor_granted", text);
            }
            ServerMessage::FloorDenied { .. } => {
                let _ = app.emit("floor_denied", text);
            }
            ServerMessage::FloorOccupied { .. } => {
                let _ = app.emit("floor_occupied", text);
            }
            ServerMessage::FloorReleased { .. } => {
                let _ = app.emit("floor_released", text);
            }
            ServerMessage::FloorTimeout { .. } => {
                let _ = app.emit("floor_timeout", text);
            }
            ServerMessage::PresenceUpdate { .. } => {
                let _ = app.emit("presence_update", text);
            }
            ServerMessage::MemberJoined { .. } => {
                let _ = app.emit("member_joined", text);
            }
            ServerMessage::MemberLeft { .. } => {
                let _ = app.emit("member_left", text);
            }
            ServerMessage::HeartbeatAck { .. } => {
                // No-op; confirms connection is alive
            }
            ServerMessage::Error { code, message } => {
                tracing::warn!("Server error {code}: {message}");
                let _ = app.emit("server_error", text);
            }
        }
    }
}
