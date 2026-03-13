pub mod auth;
pub mod health;
pub mod users;

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

/// Routes for `/auth/*` — public (no JWT required).
pub fn auth_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
        .route("/logout", post(auth::logout))
}

/// Routes for `/users/*` — all require JWT authentication via `AuthUser` extractor.
pub fn users_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/me", get(users::get_me))
        .route("/me/devices", get(users::list_devices).post(users::create_device))
        .route("/me/devices/:id", delete(users::delete_device))
}
