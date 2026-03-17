pub mod config;
pub mod models;
pub mod routes;
pub mod state;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use routes::health::health_check;
use routes::{auth_router, users_router};
use state::AppState;

/// Build the auth-service Axum router. Used by `main` and integration tests.
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .nest("/auth", auth_router())
        .nest("/users", users_router())
        .with_state(state)
}
