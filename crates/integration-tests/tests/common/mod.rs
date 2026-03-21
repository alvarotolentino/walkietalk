//! Shared test infrastructure: Redis/LuxDB testcontainer, server launchers, helpers.
//!
//! Each integration test file compiles as a separate binary crate with its own
//! copy of this module.  Functions used only by *other* test files appear
//! "unused" in each particular compilation — these are false positives.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use walkietalk_shared::auth::encode_jwt;
use walkietalk_shared::db::{self, RedisConn};
use walkietalk_shared::ids::UserId;
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

use walkietalk_auth::state::AppState as AuthState;
use walkietalk_signaling::floor::FloorManager;
use walkietalk_signaling::hub::WsHub;
use walkietalk_signaling::metrics::Metrics;
use walkietalk_signaling::presence::PresenceManager;
use walkietalk_signaling::state::AppState as SignalingState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TEST_JWT_SECRET: &str = "integration-test-secret-key-for-ci";
pub const RECV_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Redis/LuxDB testcontainer
// ---------------------------------------------------------------------------

/// A running Redis-compatible container with a connection manager.
pub struct TestDb {
    pub redis: RedisConn,
    pub redis_url: String,
    // Hold the container to keep it alive for the test duration.
    _container: ContainerAsync<testcontainers::GenericImage>,
}

impl TestDb {
    /// Start a Redis container and return a connection manager.
    pub async fn start() -> Self {
        let image = testcontainers::GenericImage::new("redis", "7-alpine")
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(6379))
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ));

        let container = image.start().await.expect("failed to start redis container");

        let host_port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to get mapped port");

        let host = container
            .get_host()
            .await
            .expect("failed to get container host");

        let redis_url = format!("redis://{host}:{host_port}");

        // Wait for Redis to accept connections
        let redis = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match db::connect(&redis_url).await {
                    Ok(c) => break c,
                    Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        })
        .await
        .expect("timed out waiting for redis to accept connections");

        Self {
            redis,
            redis_url,
            _container: container,
        }
    }
}

// ---------------------------------------------------------------------------
// Server launchers
// ---------------------------------------------------------------------------

/// Start the auth service on a random port, return the `host:port` base URL.
pub async fn start_auth_server(redis: RedisConn) -> String {
    let state = Arc::new(AuthState {
        redis,
        jwt_secret: TEST_JWT_SECRET.into(),
    });

    let app = walkietalk_auth::build_app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind auth listener");
    let addr = listener.local_addr().expect("auth local addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("auth server error");
    });

    format!("127.0.0.1:{}", addr.port())
}

/// Start the signaling service on a random port, return the `host:port` base URL.
pub async fn start_signaling_server(redis: RedisConn) -> String {
    let floor_manager = FloorManager::new(redis.clone());

    let state = Arc::new(SignalingState {
        redis,
        jwt_secret: TEST_JWT_SECRET.into(),
        ws_hub: Arc::new(WsHub::new()),
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
        lock_key_map: Arc::new(DashMap::new()),
        zmq_relay: None,
        metrics: Arc::new(Metrics::new()),
    });

    let app = walkietalk_signaling::build_app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind signaling listener");
    let addr = listener.local_addr().expect("signaling local addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("signaling server error");
    });

    format!("127.0.0.1:{}", addr.port())
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// Register a new user via the auth service REST API.
/// Returns `(access_token, refresh_token, user_id)`.
pub async fn register_user(
    auth_base: &str,
    username: &str,
    email: &str,
    password: &str,
) -> (String, String, Uuid) {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{auth_base}/auth/register"))
        .json(&serde_json::json!({
            "username": username,
            "email": email,
            "password": password,
            "display_name": username,
        }))
        .send()
        .await
        .expect("register request");

    assert_eq!(res.status(), 201, "register failed: {:?}", res.text().await);
    let body: serde_json::Value = res.json().await.expect("parse register body");
    let access_token = body["access_token"].as_str().expect("access_token").to_string();
    let refresh_token = body["refresh_token"].as_str().expect("refresh_token").to_string();
    let user_id: Uuid = body["user"]["id"].as_str().expect("user.id").parse().expect("parse uuid");
    (access_token, refresh_token, user_id)
}

/// Login via the auth service REST API.
/// Returns `(access_token, refresh_token)`.
pub async fn login_user(auth_base: &str, email: &str, password: &str) -> (String, String) {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{auth_base}/auth/login"))
        .json(&serde_json::json!({
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .expect("login request");

    assert!(res.status().is_success(), "login failed: {:?}", res.text().await);
    let body: serde_json::Value = res.json().await.expect("parse login body");
    let access_token = body["access_token"].as_str().expect("access_token").to_string();
    let refresh_token = body["refresh_token"].as_str().expect("refresh_token").to_string();
    (access_token, refresh_token)
}

// ---------------------------------------------------------------------------
// Room helpers
// ---------------------------------------------------------------------------

/// Create a room via the signaling service REST API.
pub async fn create_room(
    sig_base: &str,
    token: &str,
    name: &str,
    visibility: &str,
) -> serde_json::Value {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{sig_base}/rooms"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": name,
            "visibility": visibility,
        }))
        .send()
        .await
        .expect("create room request");

    assert_eq!(res.status(), 201, "create room failed: {:?}", res.text().await);
    res.json().await.expect("parse room body")
}

/// Join a room via REST.
pub async fn join_room(sig_base: &str, token: &str, room_id: &str) {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{sig_base}/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("join room request");

    assert!(res.status().is_success(), "join room failed: {:?}", res.text().await);
}

// ---------------------------------------------------------------------------
// WebSocket helpers
// ---------------------------------------------------------------------------

pub type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn ws_connect(base_url: &str, token: &str) -> WsStream {
    let url = format!("ws://{base_url}/ws?token={token}");
    let (stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");
    stream
}

pub async fn ws_send(ws: &mut WsStream, msg: &ClientMessage) {
    let json = serde_json::to_string(msg).expect("serialize ClientMessage");
    ws.send(Message::Text(json)).await.expect("ws send");
}

pub async fn ws_recv(ws: &mut WsStream) -> ServerMessage {
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

pub async fn ws_recv_binary(ws: &mut WsStream) -> Vec<u8> {
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

/// Receive up to `count` ServerMessages (best-effort, short timeout).
pub async fn ws_recv_many(ws: &mut WsStream, count: usize) -> Vec<ServerMessage> {
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

// ---------------------------------------------------------------------------
// Direct-DB helpers (bypass REST for speed when setting up test fixtures)
// ---------------------------------------------------------------------------

/// Insert a test user directly into Redis and return a JWT for that user.
pub async fn create_test_user_direct(redis: &RedisConn, username: &str) -> (UserId, String) {
    let record = db::create_user(
        &mut redis.clone(),
        username,
        &format!("{username}@test.local"),
        "not-a-real-hash",
        username,
    )
    .await
    .expect("insert test user");

    let uid = UserId(record.id);
    let jwt = encode_jwt(&uid, None, TEST_JWT_SECRET).expect("encode jwt");
    (uid, jwt)
}

/// Short unique suffix for test isolation (8 hex chars).
/// Kept short so that prefixed usernames stay within the 32-char validation limit.
pub fn unique_suffix() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}
