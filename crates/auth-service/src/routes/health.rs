use axum::response::IntoResponse;
use axum::Json;

/// GET /health — liveness check.
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}
