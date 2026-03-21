pub mod health;
pub mod rooms;

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;

use crate::state::AppState;

/// GET /metrics — lightweight JSON counters for benchmarking.
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.metrics.snapshot())
}

/// Room REST routes under `/rooms`.
pub fn rooms_router() -> Router<Arc<AppState>> {
    Router::new()
        // Static route first to avoid `:id` capturing "public"
        .route("/public", get(rooms::list_public_rooms))
        .route("/", get(rooms::list_rooms).post(rooms::create_room))
        .route(
            "/:id",
            get(rooms::get_room)
                .patch(rooms::update_room)
                .delete(rooms::delete_room),
        )
        .route("/:id/join", post(rooms::join_room))
        .route("/:id/invite", post(rooms::generate_invite))
        .route("/:id/leave", delete(rooms::leave_room))
}
