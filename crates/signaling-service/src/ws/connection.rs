use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

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
    let on_timeout = move || {
        // This runs when the 60s timeout fires
        let holder = timeout_state.floor_manager.force_release(&timeout_room);
        if let Some(uid) = holder {
            timeout_state.ws_hub.broadcast_to_room(
                &timeout_room,
                &ServerMessage::FloorTimeout {
                    room_id: timeout_room,
                    user_id: uid,
                },
            );
            timeout_state.ws_hub.broadcast_to_room(
                &timeout_room,
                &ServerMessage::FloorReleased {
                    room_id: timeout_room,
                    user_id: uid,
                },
            );
            // Revert presence to Online
            timeout_state.presence.set_status(
                &timeout_room,
                &uid,
                PresenceStatus::Online,
            );
            timeout_state.ws_hub.broadcast_to_room(
                &timeout_room,
                &ServerMessage::PresenceUpdate {
                    room_id: timeout_room,
                    user_id: uid,
                    status: PresenceStatus::Online,
                },
            );
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
            state.ws_hub.broadcast_to_room_except(
                &room_id,
                &user_id,
                &ServerMessage::FloorOccupied {
                    room_id,
                    speaker_id: user_id,
                    display_name: conn_state.display_name.clone(),
                },
            );
            // Update presence to Speaking
            state
                .presence
                .set_status(&room_id, &user_id, PresenceStatus::Speaking);
            state.ws_hub.broadcast_to_room(
                &room_id,
                &ServerMessage::PresenceUpdate {
                    room_id,
                    user_id,
                    status: PresenceStatus::Speaking,
                },
            );
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
    // Minimum: 16 bytes room_id UUID + some payload
    if data.len() < 17 {
        return;
    }

    // First 16 bytes = room_id UUID
    let room_uuid = match uuid::Uuid::from_slice(&data[..16]) {
        Ok(u) => u,
        Err(_) => return,
    };
    let room_id = RoomId(room_uuid);

    // Verify the sender holds the floor (in-memory, zero DB cost)
    if !state
        .floor_manager
        .is_held_by(&room_id, &conn_state.user_id)
    {
        return; // silently drop — sender is not the floor holder
    }

    // Relay to all other clients in the room
    state.ws_hub.broadcast_binary_to_room_except(
        &room_id,
        &conn_state.user_id,
        data,
    );
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
        state.ws_hub.broadcast_to_room(
            room_id,
            &ServerMessage::FloorReleased {
                room_id: *room_id,
                user_id: uid,
            },
        );
        // Revert presence to Online
        state
            .presence
            .set_status(room_id, &uid, PresenceStatus::Online);
        state.ws_hub.broadcast_to_room(
            room_id,
            &ServerMessage::PresenceUpdate {
                room_id: *room_id,
                user_id: uid,
                status: PresenceStatus::Online,
            },
        );
    }
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
