//! Shared test infrastructure: Postgres testcontainer, migrations, server launchers, helpers.
//!
//! Each integration test file compiles as a separate binary crate with its own
//! copy of this module.  Functions used only by *other* test files appear
//! "unused" in each particular compilation — these are false positives.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use sqlx::PgPool;
use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use walkietalk_shared::auth::encode_jwt;
use walkietalk_shared::ids::UserId;
use walkietalk_shared::messages::{ClientMessage, ServerMessage};

use walkietalk_auth::state::AppState as AuthState;
use walkietalk_signaling::floor::FloorManager;
use walkietalk_signaling::hub::WsHub;
use walkietalk_signaling::presence::PresenceManager;
use walkietalk_signaling::state::AppState as SignalingState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TEST_JWT_SECRET: &str = "integration-test-secret-key-for-ci";
pub const RECV_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Postgres testcontainer
// ---------------------------------------------------------------------------

/// A running Postgres container with a connection pool.
pub struct TestDb {
    pub pool: PgPool,
    pub db_url: String,
    // Hold the container to keep it alive for the test duration.
    _container: ContainerAsync<testcontainers::GenericImage>,
}

impl TestDb {
    /// Start a Postgres 16 container, run migrations, and return the pool.
    pub async fn start() -> Self {
        // GenericImage methods (with_exposed_port, with_wait_for) must be called
        // BEFORE ImageExt methods (with_env_var), because ImageExt converts the
        // type to ContainerRequest<GenericImage> which lacks GenericImage-specific methods.
        let image = testcontainers::GenericImage::new("postgres", "16-alpine")
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
            .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_env_var("POSTGRES_DB", "walkietalk_test");

        let container = image.start().await.expect("failed to start postgres container");

        let host_port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("failed to get mapped port");

        let host = container
            .get_host()
            .await
            .expect("failed to get container host");

        let db_url = format!(
            "postgres://test:test@{host}:{host_port}/walkietalk_test"
        );

        // Wait for Postgres to actually accept connections (beyond the log message)
        let pool = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match PgPool::connect(&db_url).await {
                    Ok(p) => break p,
                    Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        })
        .await
        .expect("timed out waiting for postgres to accept connections");

        // Run migrations
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("failed to run database migrations");

        Self {
            pool,
            db_url,
            _container: container,
        }
    }
}

// ---------------------------------------------------------------------------
// Server launchers
// ---------------------------------------------------------------------------

/// Start the auth service on a random port, return the `host:port` base URL.
pub async fn start_auth_server(pool: PgPool) -> String {
    let state = Arc::new(AuthState {
        db: pool,
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
pub async fn start_signaling_server(pool: PgPool, db_url: &str) -> String {
    let floor_manager = FloorManager::new(db_url, 10)
        .await
        .expect("floor manager");

    let state = Arc::new(SignalingState {
        db: pool,
        jwt_secret: TEST_JWT_SECRET.into(),
        ws_hub: Arc::new(WsHub::new()),
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
        lock_key_map: Arc::new(DashMap::new()),
        zmq_relay: None,
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

/// Insert a test user directly into the DB and return a JWT for that user.
pub async fn create_test_user_direct(pool: &PgPool, username: &str) -> (UserId, String) {
    let user_id = Uuid::new_v4();
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

/// Short unique suffix for test isolation (8 hex chars).
/// Kept short so that prefixed usernames stay within the 32-char validation limit.
pub fn unique_suffix() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}
