//! Integration tests for the signaling-service (rooms CRUD, WS connect, floor, audio relay).

mod common;

use common::*;
use walkietalk_shared::audio::{AudioFrame, FLAG_END_OF_TRANSMISSION, HEADER_SIZE};
use walkietalk_shared::ids::RoomId;
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

// ---------------------------------------------------------------------------
// Room CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_and_get_room() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (uid, jwt) = create_test_user_direct(&db.redis, &format!("alice_{s}")).await;

    let room = create_room(&sig_base, &jwt, &format!("Room_{s}"), "public").await;
    let room_id = room["id"].as_str().expect("room id");
    assert_eq!(room["name"].as_str().unwrap(), format!("Room_{s}"));
    assert_eq!(room["visibility"].as_str().unwrap(), "public");
    assert_eq!(room["owner_id"].as_str().unwrap(), uid.0.to_string());
    assert_eq!(room["member_count"].as_i64().unwrap(), 1);

    // GET /rooms/:id
    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{sig_base}/rooms/{room_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .expect("get room");

    assert_eq!(res.status(), 200);
    let detail: serde_json::Value = res.json().await.expect("parse room detail");
    assert_eq!(detail["name"].as_str().unwrap(), format!("Room_{s}"));
}

#[tokio::test]
async fn list_user_rooms() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_uid, jwt) = create_test_user_direct(&db.redis, &format!("lister_{s}")).await;

    // Create two rooms
    create_room(&sig_base, &jwt, &format!("Room1_{s}"), "private").await;
    create_room(&sig_base, &jwt, &format!("Room2_{s}"), "public").await;

    // GET /rooms (user's rooms)
    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{sig_base}/rooms"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .expect("list rooms");

    assert_eq!(res.status(), 200);
    let rooms: Vec<serde_json::Value> = res.json().await.expect("parse rooms list");
    assert_eq!(rooms.len(), 2);
}

#[tokio::test]
async fn list_public_rooms() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_uid, jwt) = create_test_user_direct(&db.redis, &format!("pub_{s}")).await;

    create_room(&sig_base, &jwt, &format!("PublicRoom_{s}"), "public").await;
    create_room(&sig_base, &jwt, &format!("PrivateRoom_{s}"), "private").await;

    // GET /rooms/public (still requires auth, just filters by visibility)
    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{sig_base}/rooms/public"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .expect("list public rooms");

    assert_eq!(res.status(), 200);
    let rooms: Vec<serde_json::Value> = res.json().await.expect("parse public rooms");
    // Only the public room should appear (test DB is isolated per container)
    assert!(rooms
        .iter()
        .any(|r| r["name"].as_str().unwrap().contains("PublicRoom")));
    assert!(!rooms
        .iter()
        .any(|r| r["name"].as_str().unwrap().contains("PrivateRoom")));
}

#[tokio::test]
async fn update_room_owner_only() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_owner, owner_jwt) = create_test_user_direct(&db.redis, &format!("owner_{s}")).await;
    let (_other, other_jwt) = create_test_user_direct(&db.redis, &format!("other_{s}")).await;

    let room = create_room(&sig_base, &owner_jwt, &format!("MyRoom_{s}"), "public").await;
    let room_id = room["id"].as_str().unwrap();

    // Other user joins
    join_room(&sig_base, &other_jwt, room_id).await;

    // Other user tries to update → should fail
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("http://{sig_base}/rooms/{room_id}"))
        .header("Authorization", format!("Bearer {other_jwt}"))
        .json(&serde_json::json!({ "name": "Hacked" }))
        .send()
        .await
        .expect("update room non-owner");

    assert_eq!(res.status(), 403, "non-owner should get 403");

    // Owner updates → should succeed
    let res = client
        .patch(format!("http://{sig_base}/rooms/{room_id}"))
        .header("Authorization", format!("Bearer {owner_jwt}"))
        .json(&serde_json::json!({ "name": format!("Updated_{s}") }))
        .send()
        .await
        .expect("update room owner");

    assert_eq!(res.status(), 200);
    let updated: serde_json::Value = res.json().await.expect("parse updated room");
    assert_eq!(updated["name"].as_str().unwrap(), format!("Updated_{s}"));
}

#[tokio::test]
async fn delete_room_owner_only() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_owner, owner_jwt) = create_test_user_direct(&db.redis, &format!("delowner_{s}")).await;
    let (_other, other_jwt) = create_test_user_direct(&db.redis, &format!("delother_{s}")).await;

    let room = create_room(&sig_base, &owner_jwt, &format!("DelRoom_{s}"), "public").await;
    let room_id = room["id"].as_str().unwrap();

    join_room(&sig_base, &other_jwt, room_id).await;

    // Non-owner delete → 403
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("http://{sig_base}/rooms/{room_id}"))
        .header("Authorization", format!("Bearer {other_jwt}"))
        .send()
        .await
        .expect("delete non-owner");

    assert_eq!(res.status(), 403);

    // Owner delete → 204
    let res = client
        .delete(format!("http://{sig_base}/rooms/{room_id}"))
        .header("Authorization", format!("Bearer {owner_jwt}"))
        .send()
        .await
        .expect("delete owner");

    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn join_private_room_with_invite_code() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_owner, owner_jwt) = create_test_user_direct(&db.redis, &format!("invown_{s}")).await;
    let (_joiner, joiner_jwt) = create_test_user_direct(&db.redis, &format!("invjoin_{s}")).await;

    let room = create_room(&sig_base, &owner_jwt, &format!("PrivRoom_{s}"), "private").await;
    let room_id = room["id"].as_str().unwrap();

    // Generate invite code
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{sig_base}/rooms/{room_id}/invite"))
        .header("Authorization", format!("Bearer {owner_jwt}"))
        .send()
        .await
        .expect("generate invite");

    assert_eq!(res.status(), 200);
    let invite_body: serde_json::Value = res.json().await.expect("parse invite");
    let invite_code = invite_body["invite_code"].as_str().expect("invite_code");

    // Join without invite → should fail
    let res = client
        .post(format!("http://{sig_base}/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {joiner_jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("join without invite");

    assert!(
        res.status().is_client_error(),
        "join private without invite should fail"
    );

    // Join with invite code → should succeed
    let res = client
        .post(format!("http://{sig_base}/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {joiner_jwt}"))
        .json(&serde_json::json!({ "invite_code": invite_code }))
        .send()
        .await
        .expect("join with invite");

    assert!(
        res.status().is_success(),
        "join with invite should succeed: {}",
        res.status()
    );
}

#[tokio::test]
async fn leave_room() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_owner, owner_jwt) = create_test_user_direct(&db.redis, &format!("leaveown_{s}")).await;
    let (_joiner, joiner_jwt) = create_test_user_direct(&db.redis, &format!("leavejoin_{s}")).await;

    let room = create_room(&sig_base, &owner_jwt, &format!("LeaveRoom_{s}"), "public").await;
    let room_id = room["id"].as_str().unwrap();

    join_room(&sig_base, &joiner_jwt, room_id).await;

    // Leave
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("http://{sig_base}/rooms/{room_id}/leave"))
        .header("Authorization", format!("Bearer {joiner_jwt}"))
        .send()
        .await
        .expect("leave room");

    assert_eq!(res.status(), 204);
}

// ---------------------------------------------------------------------------
// WebSocket: Connect, Join, Floor, Audio
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_join_room_and_floor_management() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (user_a, jwt_a) = create_test_user_direct(&db.redis, &format!("wsa_{s}")).await;
    let (_user_b, jwt_b) = create_test_user_direct(&db.redis, &format!("wsb_{s}")).await;

    // Create a room, user B joins
    let room = create_room(&sig_base, &jwt_a, &format!("WsRoom_{s}"), "public").await;
    let room_id_str = room["id"].as_str().unwrap();
    let room_id = RoomId(room_id_str.parse().unwrap());
    join_room(&sig_base, &jwt_b, room_id_str).await;

    // Connect WebSockets
    let mut ws_a = ws_connect(&sig_base, &jwt_a).await;
    let mut ws_b = ws_connect(&sig_base, &jwt_b).await;

    // A joins room via WS
    ws_send(&mut ws_a, &ClientMessage::JoinRoom { room_id }).await;
    let state_a = ws_recv(&mut ws_a).await;
    assert!(matches!(state_a, ServerMessage::RoomState { .. }));

    // B joins room via WS
    ws_send(&mut ws_b, &ClientMessage::JoinRoom { room_id }).await;
    let state_b = ws_recv(&mut ws_b).await;
    match &state_b {
        ServerMessage::RoomState { members, .. } => assert_eq!(members.len(), 2),
        other => panic!("expected RoomState, got: {other:?}"),
    }

    // Drain A's notifications (MemberJoined, PresenceUpdate)
    let _ = ws_recv_many(&mut ws_a, 3).await;

    // A requests floor
    ws_send(&mut ws_a, &ClientMessage::FloorRequest { room_id }).await;
    let granted = ws_recv(&mut ws_a).await;
    match &granted {
        ServerMessage::FloorGranted { user_id, .. } => assert_eq!(*user_id, user_a),
        other => panic!("expected FloorGranted, got: {other:?}"),
    }

    // B sees FloorOccupied
    let b_msgs = ws_recv_many(&mut ws_b, 3).await;
    assert!(
        b_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::FloorOccupied { .. })),
        "expected FloorOccupied, got: {b_msgs:?}"
    );

    // B tries floor → denied
    ws_send(&mut ws_b, &ClientMessage::FloorRequest { room_id }).await;
    let denied = ws_recv(&mut ws_b).await;
    assert!(matches!(denied, ServerMessage::FloorDenied { .. }));

    // A releases floor
    ws_send(&mut ws_a, &ClientMessage::FloorRelease { room_id }).await;

    // Both should get FloorReleased
    let a_release = ws_recv_many(&mut ws_a, 3).await;
    assert!(a_release
        .iter()
        .any(|m| matches!(m, ServerMessage::FloorReleased { .. })));

    let b_release = ws_recv_many(&mut ws_b, 3).await;
    assert!(b_release
        .iter()
        .any(|m| matches!(m, ServerMessage::FloorReleased { .. })));

    ws_a.close(None).await.ok();
    ws_b.close(None).await.ok();
}

#[tokio::test]
async fn ws_audio_relay_and_eot() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_user_a, jwt_a) = create_test_user_direct(&db.redis, &format!("auda_{s}")).await;
    let (_user_b, jwt_b) = create_test_user_direct(&db.redis, &format!("audb_{s}")).await;

    let room = create_room(&sig_base, &jwt_a, &format!("AudioRoom_{s}"), "public").await;
    let room_id_str = room["id"].as_str().unwrap();
    let room_id = RoomId(room_id_str.parse().unwrap());
    join_room(&sig_base, &jwt_b, room_id_str).await;

    let mut ws_a = ws_connect(&sig_base, &jwt_a).await;
    let mut ws_b = ws_connect(&sig_base, &jwt_b).await;

    // Both join via WS
    ws_send(&mut ws_a, &ClientMessage::JoinRoom { room_id }).await;
    let _ = ws_recv(&mut ws_a).await; // RoomState

    ws_send(&mut ws_b, &ClientMessage::JoinRoom { room_id }).await;
    let _ = ws_recv(&mut ws_b).await; // RoomState
    let _ = ws_recv_many(&mut ws_a, 3).await; // drain notifications

    // A requests floor
    ws_send(&mut ws_a, &ClientMessage::FloorRequest { room_id }).await;
    let _ = ws_recv(&mut ws_a).await; // FloorGranted
    let _ = ws_recv_many(&mut ws_b, 3).await; // FloorOccupied + presence

    // Get lock_key for wire room_id
    use walkietalk_shared::db;
    let lock_key: i64 = db::get_room_lock_key(&mut db.redis.clone(), room_id.0)
        .await
        .expect("get lock_key")
        .expect("lock_key should exist");

    // Send audio frame
    let frame = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 42,
        flags: 0,
        payload: vec![0xCA, 0xFE, 0xBA, 0xBE],
    };
    let encoded = frame.encode();
    assert_eq!(encoded.len(), HEADER_SIZE + 4);

    use futures_util::SinkExt;
    ws_a.send(tokio_tungstenite::tungstenite::Message::Binary(
        encoded.clone(),
    ))
    .await
    .expect("send audio");

    // B receives binary
    let received = ws_recv_binary(&mut ws_b).await;
    assert_eq!(received, encoded);

    let decoded = AudioFrame::decode(&received).unwrap();
    assert_eq!(decoded.room_id, lock_key as u64);
    assert_eq!(decoded.payload, vec![0xCA, 0xFE, 0xBA, 0xBE]);

    // Send EOT → auto-release floor
    let eot = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 43,
        flags: FLAG_END_OF_TRANSMISSION,
        payload: vec![],
    };
    ws_a.send(tokio_tungstenite::tungstenite::Message::Binary(
        eot.encode(),
    ))
    .await
    .expect("send EOT");

    let eot_recv = ws_recv_binary(&mut ws_b).await;
    let eot_dec = AudioFrame::decode(&eot_recv).unwrap();
    assert!(eot_dec.is_end_of_transmission());

    // Both get FloorReleased
    let a_msgs = ws_recv_many(&mut ws_a, 4).await;
    assert!(a_msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::FloorReleased { .. })));
    let b_msgs = ws_recv_many(&mut ws_b, 4).await;
    assert!(b_msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::FloorReleased { .. })));

    ws_a.close(None).await.ok();
    ws_b.close(None).await.ok();
}

#[tokio::test]
async fn ws_unauthorized_connection_rejected() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;

    // Try to connect with an invalid token
    let url = format!("ws://{sig_base}/ws?token=invalid-jwt-token");
    let result = tokio_tungstenite::connect_async(&url).await;
    // Either the connection is refused or we get an error response
    assert!(result.is_err(), "WS with invalid token should fail");
}

#[tokio::test]
async fn health_check() {
    let db = TestDb::start().await;
    let sig_base = start_signaling_server(db.redis.clone()).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{sig_base}/health"))
        .send()
        .await
        .expect("health check");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("parse health");
    assert_eq!(body["status"].as_str().unwrap(), "ok");
}
