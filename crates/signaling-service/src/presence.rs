use std::collections::HashMap;

use dashmap::DashMap;
use walkietalk_shared::enums::PresenceStatus;
use walkietalk_shared::ids::{RoomId, UserId};

/// Manages real-time presence status per user per room (in-memory).
pub struct PresenceManager {
    state: DashMap<RoomId, HashMap<UserId, PresenceStatus>>,
}

impl PresenceManager {
    pub fn new() -> Self {
        Self {
            state: DashMap::new(),
        }
    }

    /// Set a user's presence status in a room. Returns the previous status if any.
    pub fn set_status(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
        status: PresenceStatus,
    ) -> Option<PresenceStatus> {
        let mut entry = self.state.entry(*room_id).or_default();
        entry.insert(*user_id, status)
    }

    /// Get all presence statuses for a room.
    pub fn get_room_presence(&self, room_id: &RoomId) -> HashMap<UserId, PresenceStatus> {
        self.state
            .get(room_id)
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// Remove a user from a room's presence map.
    pub fn remove_user(&self, room_id: &RoomId, user_id: &UserId) {
        if let Some(mut entry) = self.state.get_mut(room_id) {
            entry.remove(user_id);
            if entry.is_empty() {
                drop(entry);
                self.state.remove(room_id);
            }
        }
    }
}

impl Default for PresenceManager {
    fn default() -> Self {
        Self::new()
    }
}
