use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use dashmap::DashMap;
use sqlx::postgres::PgPool;
use tower_http::trace::TraceLayer;

use walkietalk_signaling::config::Config;
use walkietalk_signaling::floor::FloorManager;
use walkietalk_signaling::hub::WsHub;
use walkietalk_signaling::presence::PresenceManager;
use walkietalk_signaling::routes::health::health_check;
use walkietalk_signaling::routes::rooms_router;
use walkietalk_signaling::state::AppState;
use walkietalk_signaling::ws::handler::ws_upgrade;
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
            let relay = ZmqRelay::new(push_addr, sub_addr)
                .await
                .expect("failed to connect to ZMQ proxy");
            Some(Arc::new(relay))
        }
        _ => {
            tracing::info!("ZMQ not configured — running in single-node mode");
            None
        }
    };

    let ws_hub = Arc::new(WsHub::new());
    let lock_key_map = Arc::new(DashMap::new());

    // Spawn ZMQ SUB listener if connected
    if let Some(ref relay) = zmq_relay {
        tokio::spawn(zmq_relay::zmq_sub_listener(
            Arc::clone(relay),
            Arc::clone(&ws_hub),
            Arc::clone(&lock_key_map),
        ));
    }

    let state = Arc::new(AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
        ws_hub,
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
        lock_key_map,
        zmq_relay,
    });

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws_upgrade))
        .nest("/rooms", rooms_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("signaling service listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .await
        .expect("server error");
}
