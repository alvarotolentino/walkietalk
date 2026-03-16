use std::collections::HashSet;

use axum::extract::ws::Message;
use dashmap::DashMap;
use tokio::sync::mpsc;
use walkietalk_shared::ids::{RoomId, UserId};
use walkietalk_shared::messages::ServerMessage;

/// Handle for sending messages to a connected WebSocket client.
#[derive(Clone)]
pub struct ClientHandle {
    pub user_id: UserId,
    pub tx: mpsc::UnboundedSender<Message>,
}

/// Central hub managing WebSocket client connections per room.
///
/// All methods are lock-free at the room level thanks to `DashMap`.
pub struct WsHub {
    rooms: DashMap<RoomId, Vec<ClientHandle>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
        }
    }

    /// Register a client in a room.
    pub fn add_client(&self, room_id: &RoomId, handle: ClientHandle) {
        self.rooms.entry(*room_id).or_default().push(handle);
    }

    /// Number of local clients currently in a room.
    pub fn room_client_count(&self, room_id: &RoomId) -> usize {
        self.rooms.get(room_id).map_or(0, |c| c.len())
    }

    /// Check if a user has a local connection in a given room.
    pub fn has_local_client(&self, room_id: &RoomId, user_id: &UserId) -> bool {
        self.rooms.get(room_id).is_some_and(|clients| {
            clients.iter().any(|c| c.user_id == *user_id)
        })
    }

    /// Remove a specific client from a room. Returns true if found.
    pub fn remove_client(&self, room_id: &RoomId, user_id: &UserId) -> bool {
        let mut removed = false;
        if let Some(mut clients) = self.rooms.get_mut(room_id) {
            let before = clients.len();
            clients.retain(|c| c.user_id != *user_id);
            removed = clients.len() < before;
            if clients.is_empty() {
                drop(clients);
                self.rooms.remove(room_id);
            }
        }
        removed
    }

    /// Broadcast a server message (JSON text frame) to all clients in a room.
    pub fn broadcast_to_room(&self, room_id: &RoomId, msg: &ServerMessage) {
        let json = match serde_json::to_string(msg) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("failed to serialize ServerMessage: {e}");
                return;
            }
        };
        if let Some(clients) = self.rooms.get(room_id) {
            for client in clients.iter() {
                let _ = client.tx.send(Message::Text(json.clone()));
            }
        }
    }

    /// Broadcast a server message to all clients in a room except one.
    pub fn broadcast_to_room_except(
        &self,
        room_id: &RoomId,
        exclude: &UserId,
        msg: &ServerMessage,
    ) {
        let json = match serde_json::to_string(msg) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("failed to serialize ServerMessage: {e}");
                return;
            }
        };
        if let Some(clients) = self.rooms.get(room_id) {
            for client in clients.iter() {
                if client.user_id != *exclude {
                    let _ = client.tx.send(Message::Text(json.clone()));
                }
            }
        }
    }

    /// Broadcast raw binary data to all clients in a room except one (for audio relay).
    pub fn broadcast_binary_to_room_except(
        &self,
        room_id: &RoomId,
        exclude: &UserId,
        data: &[u8],
    ) {
        if let Some(clients) = self.rooms.get(room_id) {
            for client in clients.iter() {
                if client.user_id != *exclude {
                    let _ = client.tx.send(Message::Binary(data.to_vec()));
                }
            }
        }
    }

}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-connection state tracking which rooms a client has joined.
pub struct ConnectionState {
    pub user_id: UserId,
    pub display_name: String,
    pub joined_rooms: HashSet<RoomId>,
    /// Cached lock_keys per room (room_id → lock_key) to avoid repeated DB lookups.
    pub lock_keys: std::collections::HashMap<RoomId, i64>,
    pub tx: mpsc::UnboundedSender<Message>,
}
