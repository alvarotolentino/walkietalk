use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use walkietalk_shared::audio::AudioFrame;
use walkietalk_shared::enums::PresenceStatus;
use walkietalk_shared::ids::{RoomId, UserId};
use walkietalk_shared::messages::{ClientMessage, MemberInfo, ServerMessage};

use crate::floor::get_room_lock_key;
use crate::hub::{ClientHandle, ConnectionState};
use crate::models::room::get_room_member_info;
use crate::state::AppState;

/// Maximum silence before we consider a connection dead (90 seconds).
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(90);
/// Interval at which we check for stale connections (60 seconds).
const HEARTBEAT_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Main per-connection loop. Spawned after a successful WebSocket upgrade.
pub async fn handle_connection(
    socket: WebSocket,
    user_id: UserId,
    display_name: String,
    state: Arc<AppState>,
) {
    let (ws_sink, mut ws_stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let mut conn_state = ConnectionState {
        user_id,
        display_name: display_name.clone(),
        joined_rooms: Default::default(),
        lock_keys: HashMap::new(),
        tx: tx.clone(),
    };

    tracing::info!(%user_id, "WebSocket connected");

    // Write task: forward messages from the channel to the WebSocket sink
    let mut ws_sink = ws_sink;
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Read loop with heartbeat timeout
    let mut last_activity = Instant::now();
    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_CHECK_INTERVAL);
    // Don't fire immediately on first tick
    heartbeat_interval.tick().await;

    loop {
        tokio::select! {
            maybe_msg = ws_stream.next() => {
                match maybe_msg {
                    Some(Ok(msg)) => {
                        last_activity = Instant::now();
                        match msg {
                            Message::Text(text) => {
                                handle_text_message(&text, &mut conn_state, &state).await;
                            }
                            Message::Binary(data) => {
                                handle_binary_frame(&data, &conn_state, &state).await;
                            }
                            Message::Close(_) => break,
                            // Ping/Pong handled automatically
                            _ => {}
                        }
                    }
                    Some(Err(e)) => {
                        tracing::debug!(%user_id, "WebSocket error: {e}");
                        break;
                    }
                    None => break,
                }
            }
            _ = heartbeat_interval.tick() => {
                if last_activity.elapsed() > HEARTBEAT_TIMEOUT {
                    tracing::info!(%user_id, "heartbeat timeout — closing connection");
                    break;
                }
            }
        }
    }

    // Cleanup on disconnect
    cleanup_connection(&conn_state, &state).await;
    write_task.abort();
    tracing::info!(%user_id, "WebSocket disconnected");
}

/// Parse and dispatch a text frame as a ClientMessage.
async fn handle_text_message(
    text: &str,
    conn_state: &mut ConnectionState,
    state: &Arc<AppState>,
) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!(text, "invalid ClientMessage: {e}");
            send_to_client(
                &conn_state.tx,
                &ServerMessage::Error {
                    code: 400,
                    message: "invalid message format".into(),
                },
            );
            return;
        }
    };

    match msg {
        ClientMessage::Heartbeat { ts } => {
            send_to_client(&conn_state.tx, &ServerMessage::HeartbeatAck { ts });
        }
        ClientMessage::JoinRoom { room_id } => {
            handle_join_room(room_id, conn_state, state).await;
        }
        ClientMessage::LeaveRoom { room_id } => {
            handle_leave_room(room_id, conn_state, state).await;
        }
        ClientMessage::FloorRequest { room_id } => {
            handle_floor_request(room_id, conn_state, state).await;
        }
        ClientMessage::FloorRelease { room_id } => {
            handle_floor_release(room_id, conn_state, state).await;
        }
    }
}

// ---------------------------------------------------------------------------
// JOIN_ROOM
// ---------------------------------------------------------------------------

async fn handle_join_room(
    room_id: RoomId,
    conn_state: &mut ConnectionState,
    state: &Arc<AppState>,
) {
    let user_id = conn_state.user_id;

    // Verify membership in DB
    let is_member: bool = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2)",
    )
    .bind(room_id.0)
    .bind(user_id.0)
    .fetch_one(&state.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("DB error checking membership: {e}");
            send_to_client(
                &conn_state.tx,
                &ServerMessage::Error {
                    code: 500,
                    message: "internal error".into(),
                },
            );
            return;
        }
    };

    if !is_member {
        send_to_client(
            &conn_state.tx,
            &ServerMessage::Error {
                code: 403,
                message: "not a room member".into(),
            },
        );
        return;
    }

    // Fetch member info for ROOM_STATE
    let mut members = match get_room_member_info(&state.db, &room_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("failed to get room members: {e}");
            send_to_client(
                &conn_state.tx,
                &ServerMessage::Error {
                    code: 500,
                    message: "internal error".into(),
                },
            );
            return;
        }
    };

    // Enrich with presence status
    let presence = state.presence.get_room_presence(&room_id);
    for member in &mut members {
        if let Some(status) = presence.get(&member.user_id) {
            member.status = *status;
        }
    }

    // Cache lock_key for this room (avoids DB lookup on every floor request)
    if let Ok(key) = get_room_lock_key(&state.db, &room_id).await {
        conn_state.lock_keys.insert(room_id, key);
        // Populate the shared lock_key → RoomId map for binary audio relay
        state.lock_key_map.insert(key, room_id);

        // Subscribe to ZMQ topics on first local client joining this room
        if let Some(ref relay) = state.zmq_relay {
            if state.ws_hub.room_client_count(&room_id) == 0 {
                relay.subscribe_room(key).await;
            }
        }
    }

    // Register in hub
    let handle = ClientHandle {
        user_id,
        tx: conn_state.tx.clone(),
    };
    state.ws_hub.add_client(&room_id, handle);
    conn_state.joined_rooms.insert(room_id);

    // Set presence to Online
    state
        .presence
        .set_status(&room_id, &user_id, PresenceStatus::Online);

    let floor_holder = state.floor_manager.get_holder(&room_id);

    // Send ROOM_STATE to the joining client
    send_to_client(
        &conn_state.tx,
        &ServerMessage::RoomState {
            room_id,
            members: members.clone(),
            floor_holder,
        },
    );

    // Broadcast MEMBER_JOINED + PRESENCE_UPDATE to others
    state.ws_hub.broadcast_to_room_except(
        &room_id,
        &user_id,
        &ServerMessage::MemberJoined {
            room_id,
            user: MemberInfo {
                user_id,
                display_name: conn_state.display_name.clone(),
                status: PresenceStatus::Online,
            },
        },
    );

    state.ws_hub.broadcast_to_room_except(
        &room_id,
        &user_id,
        &ServerMessage::PresenceUpdate {
            room_id,
            user_id,
            status: PresenceStatus::Online,
        },
    );

    tracing::debug!(%user_id, %room_id, "joined room via WS");
}

// ---------------------------------------------------------------------------
// LEAVE_ROOM
// ---------------------------------------------------------------------------

async fn handle_leave_room(
    room_id: RoomId,
    conn_state: &mut ConnectionState,
    state: &Arc<AppState>,
) {
    let user_id = conn_state.user_id;

    if !conn_state.joined_rooms.remove(&room_id) {
        return; // wasn't in this room
    }

    // Release floor if held
    release_floor_if_held(&room_id, &user_id, state).await;

    // Remove from hub
    state.ws_hub.remove_client(&room_id, &user_id);

    // Unsubscribe from ZMQ topics if this was the last local client
    if let Some(ref relay) = state.zmq_relay {
        if state.ws_hub.room_client_count(&room_id) == 0 {
            if let Some(&key) = conn_state.lock_keys.get(&room_id) {
                relay.unsubscribe_room(key).await;
            }
        }
    }

    // Update presence
    state.presence.remove_user(&room_id, &user_id);

    // Remove cached lock key
    conn_state.lock_keys.remove(&room_id);

    // Notify remaining members
    state.ws_hub.broadcast_to_room(
        &room_id,
        &ServerMessage::MemberLeft { room_id, user_id },
    );

    tracing::debug!(%user_id, %room_id, "left room via WS");
}

// ---------------------------------------------------------------------------
// FLOOR_REQUEST
// ---------------------------------------------------------------------------

async fn handle_floor_request(
    room_id: RoomId,
    conn_state: &mut ConnectionState,
    state: &Arc<AppState>,
) {
    let user_id = conn_state.user_id;

    if !conn_state.joined_rooms.contains(&room_id) {
        send_to_client(
            &conn_state.tx,
            &ServerMessage::Error {
                code: 400,
                message: "not in this room".into(),
            },
        );
        return;
    }

    // Check if this user already holds the floor
    if state.floor_manager.is_held_by(&room_id, &user_id) {
        send_to_client(
            &conn_state.tx,
            &ServerMessage::Error {
                code: 400,
                message: "you already hold the floor".into(),
            },
        );
        return;
    }

    // Get lock_key (from cache or DB)
    let lock_key = match conn_state.lock_keys.get(&room_id) {
        Some(&k) => k,
        None => match get_room_lock_key(&state.db, &room_id).await {
            Ok(k) => {
                conn_state.lock_keys.insert(room_id, k);
                k
            }
            Err(e) => {
                tracing::error!("failed to get lock_key: {e}");
                send_to_client(
                    &conn_state.tx,
                    &ServerMessage::FloorDenied {
                        room_id,
                        reason: "error".into(),
                    },
                );
                return;
            }
        },
    };

    // Build timeout callback
    let timeout_state = Arc::clone(state);
    let timeout_room = room_id;
    let timeout_lock_key = lock_key;
    let on_timeout = move || {
        // This runs when the 60s timeout fires
        let holder = timeout_state.floor_manager.force_release(&timeout_room);
        if let Some(uid) = holder {
            let timeout_msg = ServerMessage::FloorTimeout {
                room_id: timeout_room,
                user_id: uid,
            };
            let released_msg = ServerMessage::FloorReleased {
                room_id: timeout_room,
                user_id: uid,
            };
            let presence_msg = ServerMessage::PresenceUpdate {
                room_id: timeout_room,
                user_id: uid,
                status: PresenceStatus::Online,
            };

            timeout_state.ws_hub.broadcast_to_room(&timeout_room, &timeout_msg);
            timeout_state.ws_hub.broadcast_to_room(&timeout_room, &released_msg);

            timeout_state.presence.set_status(
                &timeout_room,
                &uid,
                PresenceStatus::Online,
            );
            timeout_state.ws_hub.broadcast_to_room(&timeout_room, &presence_msg);

            // Publish to ZMQ for other nodes
            if let Some(ref relay) = timeout_state.zmq_relay {
                let relay = Arc::clone(relay);
                let lk = timeout_lock_key;
                tokio::spawn(async move {
                    relay.publish_control(lk, &timeout_msg).await;
                    relay.publish_control(lk, &released_msg).await;
                    relay.publish_control(lk, &presence_msg).await;
                });
            }
        }
    };

    match state
        .floor_manager
        .try_acquire(room_id, lock_key, user_id, on_timeout)
        .await
    {
        Ok(true) => {
            // Floor granted
            send_to_client(
                &conn_state.tx,
                &ServerMessage::FloorGranted { room_id, user_id },
            );
            let occupied_msg = ServerMessage::FloorOccupied {
                room_id,
                speaker_id: user_id,
                display_name: conn_state.display_name.clone(),
            };
            state.ws_hub.broadcast_to_room_except(
                &room_id,
                &user_id,
                &occupied_msg,
            );
            // Update presence to Speaking
            state
                .presence
                .set_status(&room_id, &user_id, PresenceStatus::Speaking);
            let presence_msg = ServerMessage::PresenceUpdate {
                room_id,
                user_id,
                status: PresenceStatus::Speaking,
            };
            state.ws_hub.broadcast_to_room(&room_id, &presence_msg);

            // Publish floor events to ZMQ for other nodes
            if let Some(ref relay) = state.zmq_relay {
                relay.publish_control(lock_key, &occupied_msg).await;
                relay.publish_control(lock_key, &presence_msg).await;
            }
        }
        Ok(false) => {
            send_to_client(
                &conn_state.tx,
                &ServerMessage::FloorDenied {
                    room_id,
                    reason: "busy".into(),
                },
            );
        }
        Err(e) => {
            tracing::error!("floor acquire error: {e}");
            send_to_client(
                &conn_state.tx,
                &ServerMessage::FloorDenied {
                    room_id,
                    reason: "error".into(),
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// FLOOR_RELEASE
// ---------------------------------------------------------------------------

async fn handle_floor_release(
    room_id: RoomId,
    conn_state: &mut ConnectionState,
    state: &Arc<AppState>,
) {
    let user_id = conn_state.user_id;

    if !state.floor_manager.is_held_by(&room_id, &user_id) {
        send_to_client(
            &conn_state.tx,
            &ServerMessage::Error {
                code: 400,
                message: "you do not hold the floor".into(),
            },
        );
        return;
    }

    release_floor_if_held(&room_id, &user_id, state).await;
}

// ---------------------------------------------------------------------------
// Binary audio relay
// ---------------------------------------------------------------------------

async fn handle_binary_frame(
    data: &[u8],
    conn_state: &ConnectionState,
    state: &Arc<AppState>,
) {
    // Decode only the fixed header (fast path — no payload allocation)
    let (wire_room_id, speaker_id, sequence_num, flags) = match AudioFrame::decode_header(data) {
        Ok(h) => h,
        Err(_) => return, // malformed frame — silently drop
    };

    // Map wire room_id (lock_key) → RoomId UUID
    let room_id = match state.lock_key_map.get(&(wire_room_id as i64)) {
        Some(entry) => *entry,
        None => return, // unknown room — silently drop
    };

    let user_id = conn_state.user_id;

    // Floor validation (in-memory, no DB cost)
    if !state.floor_manager.is_held_by(&room_id, &user_id) {
        return; // sender does not hold the floor — silently drop
    }

    tracing::debug!(
        %room_id,
        speaker_id,
        sequence_num,
        payload_len = data.len().saturating_sub(walkietalk_shared::audio::HEADER_SIZE),
        "relaying audio frame",
    );

    // Relay verbatim binary to all other local clients in the room
    state
        .ws_hub
        .broadcast_binary_to_room_except(&room_id, &user_id, data);

    // Publish to ZMQ for multi-node fan-out (if configured)
    if let Some(ref relay) = state.zmq_relay {
        relay.publish_audio(wire_room_id as i64, &user_id, data).await;
    }

    // If END_OF_TRANSMISSION flag is set, trigger floor release
    if flags & walkietalk_shared::audio::FLAG_END_OF_TRANSMISSION != 0 {
        release_floor_if_held(&room_id, &user_id, state).await;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Serialize and send a ServerMessage to a single client.
fn send_to_client(tx: &mpsc::UnboundedSender<Message>, msg: &ServerMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = tx.send(Message::Text(json));
    }
}

/// Release the floor if held by this user, broadcast floor release + presence change.
async fn release_floor_if_held(room_id: &RoomId, user_id: &UserId, state: &Arc<AppState>) {
    if !state.floor_manager.is_held_by(room_id, user_id) {
        return;
    }

    // Use force_release (sync) to avoid needing the lock_key
    if let Some(uid) = state.floor_manager.force_release(room_id) {
        let released_msg = ServerMessage::FloorReleased {
            room_id: *room_id,
            user_id: uid,
        };
        state.ws_hub.broadcast_to_room(room_id, &released_msg);

        // Revert presence to Online
        state
            .presence
            .set_status(room_id, &uid, PresenceStatus::Online);
        let presence_msg = ServerMessage::PresenceUpdate {
            room_id: *room_id,
            user_id: uid,
            status: PresenceStatus::Online,
        };
        state.ws_hub.broadcast_to_room(room_id, &presence_msg);

        // Publish to ZMQ for other nodes
        if let Some(ref relay) = state.zmq_relay {
            // Reverse-lookup lock_key from room_id
            if let Some(lock_key) = find_lock_key(&state.lock_key_map, room_id) {
                relay.publish_control(lock_key, &released_msg).await;
                relay.publish_control(lock_key, &presence_msg).await;
            }
        }
    }
}

/// Reverse-lookup: find the lock_key for a given RoomId.
fn find_lock_key(map: &DashMap<i64, RoomId>, room_id: &RoomId) -> Option<i64> {
    map.iter().find(|entry| entry.value() == room_id).map(|entry| *entry.key())
}

/// Clean up all state when a client disconnects.
async fn cleanup_connection(conn_state: &ConnectionState, state: &Arc<AppState>) {
    let user_id = conn_state.user_id;

    for room_id in &conn_state.joined_rooms {
        // Release floor if held
        if state.floor_manager.is_held_by(room_id, &user_id) {
            if let Some(uid) = state.floor_manager.force_release(room_id) {
                state.ws_hub.broadcast_to_room(
                    room_id,
                    &ServerMessage::FloorReleased {
                        room_id: *room_id,
                        user_id: uid,
                    },
                );
            }
        }

        // Set presence to Offline and notify
        state
            .presence
            .set_status(room_id, &user_id, PresenceStatus::Offline);
        state.ws_hub.broadcast_to_room_except(
            room_id,
            &user_id,
            &ServerMessage::PresenceUpdate {
                room_id: *room_id,
                user_id,
                status: PresenceStatus::Offline,
            },
        );

        // Remove from hub
        state.ws_hub.remove_client(room_id, &user_id);

        // Unsubscribe from ZMQ if last local client left this room
        if let Some(ref relay) = state.zmq_relay {
            if state.ws_hub.room_client_count(room_id) == 0 {
                if let Some(&key) = conn_state.lock_keys.get(room_id) {
                    relay.unsubscribe_room(key).await;
                }
            }
        }

        // Broadcast member left
        state.ws_hub.broadcast_to_room(
            room_id,
            &ServerMessage::MemberLeft {
                room_id: *room_id,
                user_id,
            },
        );

        // Clean up presence entry
        state.presence.remove_user(room_id, &user_id);
    }
}
