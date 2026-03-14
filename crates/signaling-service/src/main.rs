mod config;
mod floor;
mod hub;
mod models;
mod presence;
mod routes;
mod state;
mod utils;
mod ws;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPool;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::floor::FloorManager;
use crate::hub::WsHub;
use crate::presence::PresenceManager;
use crate::routes::health::health_check;
use crate::routes::rooms_router;
use crate::state::AppState;
use crate::ws::handler::ws_upgrade;

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

    let state = Arc::new(AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
        ws_hub: Arc::new(WsHub::new()),
        floor_manager: Arc::new(floor_manager),
        presence: Arc::new(PresenceManager::new()),
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
