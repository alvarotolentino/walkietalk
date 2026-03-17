//! Cross-service integration tests: auth + signaling working together end-to-end.
//!
//! These tests exercise the full user journey:
//! 1. Register via auth-service
//! 2. Use the JWT for signaling-service operations
//! 3. WebSocket connect, floor, audio

mod common;

use common::*;
use walkietalk_shared::audio::{AudioFrame, FLAG_END_OF_TRANSMISSION};
use walkietalk_shared::ids::RoomId;
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

// ---------------------------------------------------------------------------
// Full user journey: register → create room → join → WS → PTT
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_ptt_journey_through_both_services() {
    let db = TestDb::start().await;

    let auth_base = start_auth_server(db.pool.clone()).await;
    let sig_base = start_signaling_server(db.pool.clone(), &db.db_url).await;

    let s = unique_suffix();

    // Register two users via auth service
    let (token_a, _refresh_a, _uid_a) =
        register_user(&auth_base, &format!("alice_{s}"), &format!("alice_{s}@test.io"), "password123").await;
    let (token_b, _refresh_b, _uid_b) =
        register_user(&auth_base, &format!("bob_{s}"), &format!("bob_{s}@test.io"), "password456").await;

    // Alice creates a public room via signaling service using her auth token
    let room = create_room(&sig_base, &token_a, &format!("CrossRoom_{s}"), "public").await;
    let room_id_str = room["id"].as_str().unwrap();
    let room_id = RoomId(room_id_str.parse().unwrap());

    // Bob joins the room
    join_room(&sig_base, &token_b, room_id_str).await;

    // Both connect via WebSocket
    let mut ws_a = ws_connect(&sig_base, &token_a).await;
    let mut ws_b = ws_connect(&sig_base, &token_b).await;

    // Both join via WS
    ws_send(&mut ws_a, &ClientMessage::JoinRoom { room_id }).await;
    let state_a = ws_recv(&mut ws_a).await;
    assert!(matches!(state_a, ServerMessage::RoomState { .. }));

    ws_send(&mut ws_b, &ClientMessage::JoinRoom { room_id }).await;
    let state_b = ws_recv(&mut ws_b).await;
    match &state_b {
        ServerMessage::RoomState { members, .. } => assert_eq!(members.len(), 2),
        other => panic!("expected RoomState with 2 members, got: {other:?}"),
    }

    // Drain A's join notifications
    let _ = ws_recv_many(&mut ws_a, 3).await;

    // Alice requests floor
    ws_send(&mut ws_a, &ClientMessage::FloorRequest { room_id }).await;
    let granted = ws_recv(&mut ws_a).await;
    assert!(matches!(granted, ServerMessage::FloorGranted { .. }));

    // Bob sees FloorOccupied
    let b_floor = ws_recv_many(&mut ws_b, 3).await;
    assert!(b_floor.iter().any(|m| matches!(m, ServerMessage::FloorOccupied { .. })));

    // Get lock_key for audio wire format
    let lock_key: i64 = sqlx::query_scalar("SELECT lock_key FROM rooms WHERE id = $1")
        .bind(room_id.0)
        .fetch_one(&db.pool)
        .await
        .expect("get lock_key");

    // Alice sends audio
    let frame = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 1,
        flags: 0,
        payload: vec![0xDE, 0xAD],
    };

    use futures_util::SinkExt;
    ws_a.send(tokio_tungstenite::tungstenite::Message::Binary(frame.encode()))
        .await
        .expect("send audio");

    // Bob receives audio
    let audio = ws_recv_binary(&mut ws_b).await;
    let decoded = AudioFrame::decode(&audio).unwrap();
    assert_eq!(decoded.payload, vec![0xDE, 0xAD]);

    // Alice sends EOT
    let eot = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 2,
        flags: FLAG_END_OF_TRANSMISSION,
        payload: vec![],
    };
    ws_a.send(tokio_tungstenite::tungstenite::Message::Binary(eot.encode()))
        .await
        .expect("send EOT");

    let eot_recv = ws_recv_binary(&mut ws_b).await;
    let eot_dec = AudioFrame::decode(&eot_recv).unwrap();
    assert!(eot_dec.is_end_of_transmission());

    ws_a.close(None).await.ok();
    ws_b.close(None).await.ok();
}

// ---------------------------------------------------------------------------
// Auth token from one service valid in the other
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auth_token_works_across_services() {
    let db = TestDb::start().await;

    let auth_base = start_auth_server(db.pool.clone()).await;
    let sig_base = start_signaling_server(db.pool.clone(), &db.db_url).await;

    let s = unique_suffix();

    // Register via auth service
    let (token, _refresh, user_id) =
        register_user(&auth_base, &format!("cross_{s}"), &format!("cross_{s}@test.io"), "password123").await;

    // Use auth token to GET /users/me
    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{auth_base}/users/me"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("get me via auth");
    assert_eq!(res.status(), 200);

    // Use same token to create a room via signaling
    let room = create_room(&sig_base, &token, &format!("CrossCheck_{s}"), "public").await;
    assert_eq!(room["owner_id"].as_str().unwrap(), user_id.to_string());

    // Use same token for WebSocket
    let mut ws = ws_connect(&sig_base, &token).await;
    let room_id = RoomId(room["id"].as_str().unwrap().parse().unwrap());
    ws_send(&mut ws, &ClientMessage::JoinRoom { room_id }).await;
    let state = ws_recv(&mut ws).await;
    assert!(matches!(state, ServerMessage::RoomState { .. }));

    ws.close(None).await.ok();
}

// ---------------------------------------------------------------------------
// Refresh token flow then use new token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_then_use_new_token() {
    let db = TestDb::start().await;

    let auth_base = start_auth_server(db.pool.clone()).await;
    let sig_base = start_signaling_server(db.pool.clone(), &db.db_url).await;

    let s = unique_suffix();

    let (_token, refresh, _uid) =
        register_user(&auth_base, &format!("refresh_{s}"), &format!("refresh_{s}@test.io"), "password123").await;

    // Refresh to get a new access token
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{auth_base}/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .expect("refresh");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("parse refresh");
    let new_token = body["access_token"].as_str().unwrap();

    // Use new token to create a room in signaling service
    let room = create_room(&sig_base, new_token, &format!("RefreshRoom_{s}"), "public").await;
    assert!(!room["id"].as_str().unwrap().is_empty());
}
