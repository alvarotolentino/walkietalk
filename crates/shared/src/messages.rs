use serde::{Deserialize, Serialize};

use crate::enums::PresenceStatus;
use crate::ids::{RoomId, UserId};

// ---------------------------------------------------------------------------
// Client → Server
// ---------------------------------------------------------------------------

/// Control messages sent from the client to the server over WebSocket text frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMessage {
    JoinRoom { room_id: RoomId },
    LeaveRoom { room_id: RoomId },
    FloorRequest { room_id: RoomId },
    FloorRelease { room_id: RoomId },
    Heartbeat { ts: i64 },
}

// ---------------------------------------------------------------------------
// Server → Client
// ---------------------------------------------------------------------------

/// Control messages sent from the server to the client over WebSocket text frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    RoomState {
        room_id: RoomId,
        members: Vec<MemberInfo>,
        floor_holder: Option<UserId>,
    },
    FloorGranted {
        room_id: RoomId,
        user_id: UserId,
    },
    FloorDenied {
        room_id: RoomId,
        reason: String,
    },
    FloorOccupied {
        room_id: RoomId,
        speaker_id: UserId,
        display_name: String,
    },
    FloorReleased {
        room_id: RoomId,
        user_id: UserId,
    },
    FloorTimeout {
        room_id: RoomId,
        user_id: UserId,
    },
    PresenceUpdate {
        room_id: RoomId,
        user_id: UserId,
        status: PresenceStatus,
    },
    MemberJoined {
        room_id: RoomId,
        user: MemberInfo,
    },
    MemberLeft {
        room_id: RoomId,
        user_id: UserId,
    },
    Error {
        code: u16,
        message: String,
    },
    HeartbeatAck {
        ts: i64,
    },
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Summary of a room member, included in room state and join/leave events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberInfo {
    pub user_id: UserId,
    pub display_name: String,
    pub status: PresenceStatus,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn client_floor_request_serializes_correctly() {
        let room_id = RoomId(Uuid::nil());
        let msg = ClientMessage::FloorRequest { room_id };
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["type"], "FLOOR_REQUEST");
        assert_eq!(json["room_id"], Uuid::nil().to_string());
    }

    #[test]
    fn server_floor_granted_deserializes_correctly() {
        let json = serde_json::json!({
            "type": "FLOOR_GRANTED",
            "room_id": "00000000-0000-0000-0000-000000000000",
            "user_id": "00000000-0000-0000-0000-000000000001"
        });
        let msg: ServerMessage = serde_json::from_value(json).expect("deserialize");
        match msg {
            ServerMessage::FloorGranted { room_id, user_id } => {
                assert_eq!(room_id, RoomId(Uuid::nil()));
                assert_eq!(
                    user_id,
                    UserId(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap())
                );
            }
            other => panic!("expected FloorGranted, got {other:?}"),
        }
    }

    #[test]
    fn client_message_roundtrip() {
        let room_id = RoomId(Uuid::new_v4());
        let messages = vec![
            ClientMessage::JoinRoom { room_id },
            ClientMessage::LeaveRoom { room_id },
            ClientMessage::FloorRequest { room_id },
            ClientMessage::FloorRelease { room_id },
            ClientMessage::Heartbeat { ts: 1_710_000_000 },
        ];
        for msg in &messages {
            let json = serde_json::to_string(msg).expect("serialize");
            let deserialized: ClientMessage = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&deserialized, msg);
        }
    }

    #[test]
    fn server_message_roundtrip() {
        let room_id = RoomId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());
        let messages: Vec<ServerMessage> = vec![
            ServerMessage::RoomState {
                room_id,
                members: vec![MemberInfo {
                    user_id,
                    display_name: "Alice".into(),
                    status: PresenceStatus::Online,
                }],
                floor_holder: None,
            },
            ServerMessage::FloorGranted { room_id, user_id },
            ServerMessage::FloorDenied {
                room_id,
                reason: "busy".into(),
            },
            ServerMessage::FloorOccupied {
                room_id,
                speaker_id: user_id,
                display_name: "Alice".into(),
            },
            ServerMessage::FloorReleased { room_id, user_id },
            ServerMessage::FloorTimeout { room_id, user_id },
            ServerMessage::PresenceUpdate {
                room_id,
                user_id,
                status: PresenceStatus::Speaking,
            },
            ServerMessage::MemberJoined {
                room_id,
                user: MemberInfo {
                    user_id,
                    display_name: "Bob".into(),
                    status: PresenceStatus::Online,
                },
            },
            ServerMessage::MemberLeft { room_id, user_id },
            ServerMessage::Error {
                code: 400,
                message: "bad".into(),
            },
            ServerMessage::HeartbeatAck { ts: 42 },
        ];
        for msg in &messages {
            let json = serde_json::to_string(msg).expect("serialize");
            let deserialized: ServerMessage = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&deserialized, msg);
        }
    }
}
