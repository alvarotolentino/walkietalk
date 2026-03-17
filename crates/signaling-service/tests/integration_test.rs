use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use walkietalk_shared::audio::{AudioFrame, FLAG_END_OF_TRANSMISSION, HEADER_SIZE};
use walkietalk_shared::auth::encode_jwt;
use walkietalk_shared::ids::{RoomId, UserId};
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

/// Bring in the signaling crate's internals for building the server.
use walkietalk_signaling::floor::FloorManager;
use walkietalk_signaling::hub::WsHub;
use walkietalk_signaling::presence::PresenceManager;
use walkietalk_signaling::state::AppState;

const TEST_JWT_SECRET: &str = "integration-test-secret";
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Helper: spin up the server, return the base URL
// ---------------------------------------------------------------------------

async fn start_server(pool: PgPool) -> String {
    let floor_manager = FloorManager::new(
        &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
        10,
    )
    .await
    .expect("floor manager");

    let state = Arc::new(AppState {
        db: pool,
        jwt_secret: TEST_JWT_SECRET.into(),
        ws_hub: Arc::new(WsHub::new()),
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
        lock_key_map: Arc::new(DashMap::new()),
        zmq_relay: None,
    });

    let app = walkietalk_signaling::build_app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("127.0.0.1:{}", addr.port())
}

// ---------------------------------------------------------------------------
// Helper: create a test user directly in the DB, return (UserId, JWT)
// ---------------------------------------------------------------------------

async fn create_test_user(pool: &PgPool, username: &str) -> (UserId, String) {
    let user_id = Uuid::new_v4();
    // password_hash is irrelevant for WS tests — just needs to be non-empty
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@test.local"))
    .bind("not-a-real-hash")
    .bind(username)
    .execute(pool)
    .await
    .expect("insert test user");

    let uid = UserId(user_id);
    let jwt = encode_jwt(&uid, None, TEST_JWT_SECRET).expect("encode jwt");
    (uid, jwt)
}

// ---------------------------------------------------------------------------
// Helper: create a public room via REST, return its RoomId
// ---------------------------------------------------------------------------

async fn create_test_room(base_url: &str, token: &str, name: &str) -> RoomId {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base_url}/rooms"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": name,
            "visibility": "public"
        }))
        .send()
        .await
        .expect("create room request");

    assert_eq!(res.status(), 201, "create room failed: {}", res.status());
    let body: serde_json::Value = res.json().await.expect("parse room body");
    let id_str = body["id"].as_str().expect("room id");
    RoomId(id_str.parse().expect("parse room uuid"))
}

// ---------------------------------------------------------------------------
// Helper: have a user join a room via REST
// ---------------------------------------------------------------------------

async fn join_room_rest(base_url: &str, token: &str, room_id: &RoomId) {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base_url}/rooms/{}/join", room_id.0))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("join room request");

    assert!(
        res.status().is_success(),
        "join room failed: {}",
        res.status()
    );
}

// ---------------------------------------------------------------------------
// Helper: connect a WebSocket client
// ---------------------------------------------------------------------------

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn ws_connect(base_url: &str, token: &str) -> WsStream {
    let url = format!("ws://{base_url}/ws?token={token}");
    let (stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");
    stream
}

// ---------------------------------------------------------------------------
// Helper: send a ClientMessage as JSON text
// ---------------------------------------------------------------------------

async fn ws_send(ws: &mut WsStream, msg: &ClientMessage) {
    let json = serde_json::to_string(msg).unwrap();
    ws.send(Message::Text(json)).await.expect("ws send");
}

// ---------------------------------------------------------------------------
// Helper: receive a ServerMessage (text frame), with timeout
// ---------------------------------------------------------------------------

async fn ws_recv(ws: &mut WsStream) -> ServerMessage {
    loop {
        let msg = timeout(RECV_TIMEOUT, ws.next())
            .await
            .expect("ws recv timed out")
            .expect("ws stream ended")
            .expect("ws recv error");

        match msg {
            Message::Text(text) => {
                return serde_json::from_str(&text).unwrap_or_else(|e| {
                    panic!("failed to parse ServerMessage: {e}\nraw: {text}")
                });
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected ws message type: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: receive a binary frame, with timeout
// ---------------------------------------------------------------------------

async fn ws_recv_binary(ws: &mut WsStream) -> Vec<u8> {
    loop {
        let msg = timeout(RECV_TIMEOUT, ws.next())
            .await
            .expect("ws recv binary timed out")
            .expect("ws stream ended")
            .expect("ws recv binary error");

        match msg {
            Message::Binary(data) => return data,
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Text(text) => panic!("expected binary, got text: {text}"),
            other => panic!("unexpected ws message type: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: drain all pending text messages, returning the last N
// ---------------------------------------------------------------------------

/// Receive up to `count` ServerMessages (best-effort, short timeout).
async fn ws_recv_many(ws: &mut WsStream, count: usize) -> Vec<ServerMessage> {
    let mut msgs = Vec::with_capacity(count);
    for _ in 0..count {
        match timeout(Duration::from_millis(500), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(msg) = serde_json::from_str(&text) {
                    msgs.push(msg);
                }
            }
            _ => break,
        }
    }
    msgs
}

// ===========================================================================
// Integration test
// ===========================================================================

#[tokio::test]
async fn test_two_clients_ptt_audio_exchange() {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPool::connect(&db_url).await.expect("connect to DB");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let base = start_server(pool.clone()).await;

    // ── Create two test users ──────────────────────────────────────────────
    let suffix = Uuid::new_v4().simple().to_string();
    let (user_a, jwt_a) = create_test_user(&pool, &format!("alice_{suffix}")).await;
    let (_user_b, jwt_b) = create_test_user(&pool, &format!("bob_{suffix}")).await;

    // ── User A creates a public room ───────────────────────────────────────
    let room_id = create_test_room(&base, &jwt_a, &format!("TestRoom_{suffix}")).await;

    // ── User B joins the room via REST ─────────────────────────────────────
    join_room_rest(&base, &jwt_b, &room_id).await;

    // ── Both connect via WebSocket ─────────────────────────────────────────
    let mut ws_a = ws_connect(&base, &jwt_a).await;
    let mut ws_b = ws_connect(&base, &jwt_b).await;

    // ── Client A joins room via WS ─────────────────────────────────────────
    ws_send(&mut ws_a, &ClientMessage::JoinRoom { room_id }).await;
    let room_state_a = ws_recv(&mut ws_a).await;
    match &room_state_a {
        ServerMessage::RoomState { room_id: rid, .. } => assert_eq!(*rid, room_id),
        other => panic!("expected RoomState, got: {other:?}"),
    }

    // ── Client B joins room via WS ─────────────────────────────────────────
    ws_send(&mut ws_b, &ClientMessage::JoinRoom { room_id }).await;
    let room_state_b = ws_recv(&mut ws_b).await;
    match &room_state_b {
        ServerMessage::RoomState { room_id: rid, members, .. } => {
            assert_eq!(*rid, room_id);
            assert_eq!(members.len(), 2); // A and B
        }
        other => panic!("expected RoomState, got: {other:?}"),
    }

    // Client A receives MEMBER_JOINED + PRESENCE_UPDATE for B
    let a_notifications = ws_recv_many(&mut ws_a, 3).await;
    assert!(
        a_notifications.iter().any(|m| matches!(m, ServerMessage::MemberJoined { .. })),
        "expected MEMBER_JOINED in: {a_notifications:?}"
    );

    // ── Client A requests floor ────────────────────────────────────────────
    ws_send(&mut ws_a, &ClientMessage::FloorRequest { room_id }).await;

    let floor_granted = ws_recv(&mut ws_a).await;
    match &floor_granted {
        ServerMessage::FloorGranted { room_id: rid, user_id } => {
            assert_eq!(*rid, room_id);
            assert_eq!(*user_id, user_a);
        }
        other => panic!("expected FloorGranted, got: {other:?}"),
    }

    // Client B receives FLOOR_OCCUPIED + PRESENCE_UPDATE(Speaking)
    let b_floor_msgs = ws_recv_many(&mut ws_b, 3).await;
    assert!(
        b_floor_msgs.iter().any(|m| matches!(m, ServerMessage::FloorOccupied { .. })),
        "expected FLOOR_OCCUPIED in: {b_floor_msgs:?}"
    );

    // ── Client B also tries to request floor → denied ──────────────────────
    ws_send(&mut ws_b, &ClientMessage::FloorRequest { room_id }).await;
    let denied = ws_recv(&mut ws_b).await;
    match &denied {
        ServerMessage::FloorDenied { room_id: rid, reason } => {
            assert_eq!(*rid, room_id);
            assert_eq!(reason, "busy");
        }
        other => panic!("expected FloorDenied, got: {other:?}"),
    }

    // ── Client A sends a binary audio frame ────────────────────────────────
    // We need the room's lock_key for the wire room_id. Look it up:
    let lock_key: i64 = sqlx::query_scalar("SELECT lock_key FROM rooms WHERE id = $1")
        .bind(room_id.0)
        .fetch_one(&pool)
        .await
        .expect("get lock_key");

    let frame = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 42,
        flags: 0,
        payload: vec![0xCA, 0xFE, 0xBA, 0xBE],
    };
    let encoded = frame.encode();
    assert_eq!(encoded.len(), HEADER_SIZE + 4);

    ws_a.send(Message::Binary(encoded.clone()))
        .await
        .expect("send audio frame");

    // ── Client B receives the binary audio frame ───────────────────────────
    let received = ws_recv_binary(&mut ws_b).await;
    assert_eq!(received, encoded, "relayed frame must be verbatim");

    // Decode and verify the header
    let decoded = AudioFrame::decode(&received).unwrap();
    assert_eq!(decoded.room_id, lock_key as u64);
    assert_eq!(decoded.speaker_id, 1);
    assert_eq!(decoded.sequence_num, 42);
    assert_eq!(decoded.payload, vec![0xCA, 0xFE, 0xBA, 0xBE]);

    // ── Client A sends EOT frame → floor should auto-release ──────────────
    let eot_frame = AudioFrame {
        room_id: lock_key as u64,
        speaker_id: 1,
        sequence_num: 43,
        flags: FLAG_END_OF_TRANSMISSION,
        payload: vec![],
    };
    ws_a.send(Message::Binary(eot_frame.encode()))
        .await
        .expect("send EOT frame");

    // Client B receives the EOT binary frame
    let eot_received = ws_recv_binary(&mut ws_b).await;
    let eot_decoded = AudioFrame::decode(&eot_received).unwrap();
    assert!(eot_decoded.is_end_of_transmission());

    // Both should receive FLOOR_RELEASED (A broadcast, B broadcast)
    // Client A gets it:
    let a_release_msgs = ws_recv_many(&mut ws_a, 4).await;
    assert!(
        a_release_msgs.iter().any(|m| matches!(m, ServerMessage::FloorReleased { .. })),
        "expected FLOOR_RELEASED for A in: {a_release_msgs:?}"
    );

    // Client B gets it:
    let b_release_msgs = ws_recv_many(&mut ws_b, 4).await;
    assert!(
        b_release_msgs.iter().any(|m| matches!(m, ServerMessage::FloorReleased { .. })),
        "expected FLOOR_RELEASED for B in: {b_release_msgs:?}"
    );

    // ── Cleanup: close both WS connections ─────────────────────────────────
    ws_a.close(None).await.ok();
    ws_b.close(None).await.ok();
}
