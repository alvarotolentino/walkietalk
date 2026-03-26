use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{mpsc, Mutex};
use zeromq::{PushSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

use walkietalk_shared::ids::{RoomId, UserId};
use walkietalk_shared::messages::ServerMessage;

use crate::hub::WsHub;

/// Topic prefixes used on the ZMQ bus.
const AUDIO_TOPIC_PREFIX: &str = "audio.";
const CTRL_TOPIC_PREFIX: &str = "ctrl.";

/// Commands sent to the SUB socket task to subscribe/unsubscribe topics
/// without deadlocking the recv loop.
pub enum SubCommand {
    Subscribe(String),
    Unsubscribe(String),
}

/// ZeroMQ relay for multi-node fan-out.
///
/// Uses PUSH/SUB pattern (adapted from XPUB/XSUB — see zmq-proxy docs):
/// - PUSH socket → connects to proxy's PULL address (publishes frames)
/// - SUB socket → connects to proxy's PUB address (receives fan-out frames)
///
/// Wire format for audio messages (3 frames):
///   Frame 0: topic string (`audio.{lock_key}`)
///   Frame 1: speaker UserId bytes (16 bytes, for exclusion on receiving nodes)
///   Frame 2: raw audio frame (binary, spec §6.6)
///
/// Wire format for control messages (2 frames):
///   Frame 0: topic string (`ctrl.{lock_key}`)
///   Frame 1: JSON-encoded ServerMessage
pub struct ZmqRelay {
    push: Mutex<PushSocket>,
    sub_cmd_tx: mpsc::UnboundedSender<SubCommand>,
}

impl ZmqRelay {
    /// Connect to the ZMQ proxy.
    ///
    /// - `push_addr`: proxy's PULL bind address (e.g. `tcp://127.0.0.1:5559`)
    /// - `sub_addr`: proxy's PUB bind address (e.g. `tcp://127.0.0.1:5560`)
    pub async fn new(
        push_addr: &str,
        sub_addr: &str,
    ) -> Result<(Self, SubSocket), Box<dyn std::error::Error>> {
        let mut push = PushSocket::new();
        push.connect(push_addr).await?;
        tracing::info!("ZMQ PUSH connected to {push_addr}");

        let mut sub = SubSocket::new();
        sub.connect(sub_addr).await?;
        tracing::info!("ZMQ SUB connected to {sub_addr}");

        let (sub_cmd_tx, _) = mpsc::unbounded_channel();

        Ok((
            Self {
                push: Mutex::new(push),
                sub_cmd_tx,
            },
            sub,
        ))
    }

    /// Replace the command channel sender (call after spawning the listener).
    pub fn set_sub_cmd_tx(&mut self, tx: mpsc::UnboundedSender<SubCommand>) {
        self.sub_cmd_tx = tx;
    }

    /// Subscribe to audio + control topics for a room.
    pub async fn subscribe_room(&self, lock_key: i64) {
        let audio_topic = format!("{AUDIO_TOPIC_PREFIX}{lock_key}");
        let ctrl_topic = format!("{CTRL_TOPIC_PREFIX}{lock_key}");
        let _ = self
            .sub_cmd_tx
            .send(SubCommand::Subscribe(audio_topic.clone()));
        let _ = self
            .sub_cmd_tx
            .send(SubCommand::Subscribe(ctrl_topic.clone()));
        tracing::debug!("ZMQ subscribe requested: {audio_topic}, {ctrl_topic}");
    }

    /// Unsubscribe from audio + control topics for a room.
    pub async fn unsubscribe_room(&self, lock_key: i64) {
        let audio_topic = format!("{AUDIO_TOPIC_PREFIX}{lock_key}");
        let ctrl_topic = format!("{CTRL_TOPIC_PREFIX}{lock_key}");
        let _ = self
            .sub_cmd_tx
            .send(SubCommand::Unsubscribe(audio_topic.clone()));
        let _ = self
            .sub_cmd_tx
            .send(SubCommand::Unsubscribe(ctrl_topic.clone()));
        tracing::debug!("ZMQ unsubscribe requested: {audio_topic}, {ctrl_topic}");
    }

    /// Publish a binary audio frame to the ZMQ bus.
    ///
    /// Includes the speaker's `UserId` as a separate frame so receiving nodes
    /// can exclude the speaker from local broadcast.
    pub async fn publish_audio(&self, lock_key: i64, speaker: &UserId, raw_frame: &[u8]) {
        let topic = format!("{AUDIO_TOPIC_PREFIX}{lock_key}");
        let mut msg = ZmqMessage::from(Bytes::from(topic.into_bytes()));
        msg.push_back(Bytes::copy_from_slice(speaker.0.as_bytes()));
        msg.push_back(Bytes::copy_from_slice(raw_frame));
        let mut push = self.push.lock().await;
        if let Err(e) = push.send(msg).await {
            tracing::warn!("ZMQ publish audio failed: {e}");
        }
    }

    /// Publish a control event (ServerMessage JSON) with topic prefix.
    pub async fn publish_control(&self, lock_key: i64, msg: &ServerMessage) {
        let topic = format!("{CTRL_TOPIC_PREFIX}{lock_key}");
        let json = match serde_json::to_vec(msg) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("failed to serialize control message for ZMQ: {e}");
                return;
            }
        };
        let mut zmq_msg = ZmqMessage::from(Bytes::from(topic.into_bytes()));
        zmq_msg.push_back(Bytes::from(json));
        let mut push = self.push.lock().await;
        if let Err(e) = push.send(zmq_msg).await {
            tracing::warn!("ZMQ publish control failed: {e}");
        }
    }
}

/// Background task: read from the ZMQ SUB socket and deliver frames
/// to local WsHub clients.
///
/// Per spec §5.3: each node's SUB socket receives the frame (via the proxy)
/// and fans it out to all local client connections in the room.
pub async fn zmq_sub_listener(
    mut sub: SubSocket,
    sub_cmd_rx: mpsc::UnboundedReceiver<SubCommand>,
    ws_hub: Arc<WsHub>,
    lock_key_map: Arc<dashmap::DashMap<i64, RoomId>>,
) {
    tracing::info!("ZMQ SUB listener started");

    let mut sub_cmd_rx = sub_cmd_rx;

    loop {
        tokio::select! {
            // Process subscribe/unsubscribe commands from connection handlers
            cmd = sub_cmd_rx.recv() => {
                match cmd {
                    Some(SubCommand::Subscribe(topic)) => {
                        sub.subscribe(&topic).await.ok();
                        tracing::debug!("ZMQ subscribed to topic: {topic}");
                    }
                    Some(SubCommand::Unsubscribe(topic)) => {
                        sub.unsubscribe(&topic).await.ok();
                        tracing::debug!("ZMQ unsubscribed from topic: {topic}");
                    }
                    None => {
                        tracing::info!("ZMQ SUB command channel closed, stopping listener");
                        break;
                    }
                }
            }
            // Receive messages from ZMQ
            result = sub.recv() => {
                match result {
                    Ok(msg) => {
                        let mut frames = msg.into_vecdeque();
                        let topic_bytes = match frames.pop_front() {
                            Some(b) => b,
                            None => continue,
                        };
                        let topic = String::from_utf8_lossy(&topic_bytes);

                        if let Some(lock_key_str) = topic.strip_prefix(AUDIO_TOPIC_PREFIX) {
                            // Audio: 3 frames — topic, speaker_uuid, raw_frame
                            let speaker_bytes = match frames.pop_front() {
                                Some(b) => b,
                                None => continue,
                            };
                            let raw_frame = match frames.pop_front() {
                                Some(b) => b,
                                None => continue,
                            };
                            handle_zmq_audio(lock_key_str, &speaker_bytes, &raw_frame, &ws_hub, &lock_key_map);
                        } else if let Some(lock_key_str) = topic.strip_prefix(CTRL_TOPIC_PREFIX) {
                            // Control: 2 frames — topic, json
                            let json_bytes = match frames.pop_front() {
                                Some(b) => b,
                                None => continue,
                            };
                            handle_zmq_control(lock_key_str, &json_bytes, &ws_hub, &lock_key_map);
                        } else {
                            tracing::debug!("ZMQ unknown topic: {topic}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("ZMQ SUB recv error: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        }
    }
}

/// Handle an audio frame received from ZMQ: broadcast binary to local clients
/// except the original speaker.
///
/// If the speaker is a local client, the frame was already broadcast directly
/// in `handle_binary_frame` — skip to avoid duplicates.
fn handle_zmq_audio(
    lock_key_str: &str,
    speaker_bytes: &[u8],
    raw_frame: &[u8],
    ws_hub: &WsHub,
    lock_key_map: &dashmap::DashMap<i64, RoomId>,
) {
    let lock_key: i64 = match lock_key_str.parse() {
        Ok(k) => k,
        Err(_) => return,
    };

    let room_id = match lock_key_map.get(&lock_key) {
        Some(entry) => *entry,
        None => return,
    };

    let speaker_uuid = match uuid::Uuid::from_slice(speaker_bytes) {
        Ok(u) => u,
        Err(_) => return,
    };
    let speaker = UserId(speaker_uuid);

    // Skip: speaker is local → local broadcast already happened
    if ws_hub.has_local_client(&room_id, &speaker) {
        return;
    }

    ws_hub.broadcast_binary_to_room_except(&room_id, &speaker, raw_frame);
}

/// Handle a control message received from ZMQ: broadcast JSON to local clients.
fn handle_zmq_control(
    lock_key_str: &str,
    json_payload: &[u8],
    ws_hub: &WsHub,
    lock_key_map: &dashmap::DashMap<i64, RoomId>,
) {
    let lock_key: i64 = match lock_key_str.parse() {
        Ok(k) => k,
        Err(_) => return,
    };

    let room_id = match lock_key_map.get(&lock_key) {
        Some(entry) => *entry,
        None => return,
    };

    let msg: ServerMessage = match serde_json::from_slice(json_payload) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("ZMQ ctrl deserialization failed: {e}");
            return;
        }
    };

    ws_hub.broadcast_to_room(&room_id, &msg);
}
