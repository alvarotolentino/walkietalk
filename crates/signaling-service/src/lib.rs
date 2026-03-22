pub mod config;
pub mod floor;
pub mod hub;
pub mod metrics;
pub mod models;
pub mod presence;
pub mod routes;
pub mod state;
pub mod utils;
pub mod ws;
pub mod zmq_relay;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use routes::health::health_check;
use routes::rooms_router;
use state::AppState;
use ws::handler::ws_upgrade;

/// Build the signaling-service Axum router. Used by `main` and integration tests.
pub fn build_app(state: Arc<AppState>) -> Router {
    let router = Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws_upgrade))
        .nest("/rooms", rooms_router());

    // Expose /metrics only when compiled with the `metrics` feature
    #[cfg(feature = "metrics")]
    let router = router.route("/metrics", get(routes::metrics_handler));

    router.with_state(state)
}
