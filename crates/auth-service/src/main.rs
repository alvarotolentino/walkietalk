mod config;
mod models;
mod routes;
mod state;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPool;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::routes::health::health_check;
use crate::routes::{auth_router, users_router};
use crate::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,walkietalk_auth=debug".parse().expect("valid filter")),
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

    let state = Arc::new(AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
    });

    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/auth", auth_router())
        .nest("/users", users_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("auth service listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .await
        .expect("server error");
}
