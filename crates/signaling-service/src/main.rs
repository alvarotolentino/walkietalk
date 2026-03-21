use std::sync::Arc;

use dashmap::DashMap;
use sqlx::postgres::PgPool;
use tower_http::trace::TraceLayer;

use walkietalk_signaling::config::Config;
use walkietalk_signaling::floor::FloorManager;
use walkietalk_signaling::hub::WsHub;
use walkietalk_signaling::metrics::Metrics;
use walkietalk_signaling::presence::PresenceManager;
use walkietalk_signaling::state::AppState;
use walkietalk_signaling::zmq_relay::{self, ZmqRelay};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,walkietalk_signaling=debug".parse().expect("valid filter")),
        )
        .init();

    let config = Config::from_env();

    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("failed to connect to database");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("failed to run database migrations");

    // Dedicated pool for floor advisory locks (max 100 connections, per spec)
    let floor_manager = FloorManager::new(&config.database_url, 100)
        .await
        .expect("failed to create floor manager");

    // Optionally connect to ZMQ fan-out proxy
    let zmq_relay = match (&config.zmq_push_addr, &config.zmq_sub_addr) {
        (Some(push_addr), Some(sub_addr)) => {
            let (relay, sub_socket) = ZmqRelay::new(push_addr, sub_addr)
                .await
                .expect("failed to connect to ZMQ proxy");
            Some((Arc::new(relay), sub_socket))
        }
        _ => {
            tracing::info!("ZMQ not configured — running in single-node mode");
            None
        }
    };

    let ws_hub = Arc::new(WsHub::new());
    let lock_key_map = Arc::new(DashMap::new());

    // Spawn ZMQ SUB listener if connected
    let zmq_relay = if let Some((mut relay, sub_socket)) = zmq_relay {
        let (sub_cmd_tx, sub_cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        Arc::get_mut(&mut relay).unwrap().set_sub_cmd_tx(sub_cmd_tx);
        tokio::spawn(zmq_relay::zmq_sub_listener(
            sub_socket,
            sub_cmd_rx,
            Arc::clone(&ws_hub),
            Arc::clone(&lock_key_map),
        ));
        Some(relay)
    } else {
        None
    };

    let state = Arc::new(AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
        ws_hub,
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
        lock_key_map,
        zmq_relay,
        metrics: Arc::new(Metrics::new()),
    });

    let app = walkietalk_signaling::build_app(state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("signaling service listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .await
        .expect("server error");
}
