use std::sync::Arc;

use tower_http::trace::TraceLayer;

use walkietalk_auth::config::Config;
use walkietalk_auth::state::AppState;
use walkietalk_shared::db;

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

    let redis = db::connect(&config.redis_url)
        .await
        .expect("failed to connect to Redis/LuxDB");

    let state = Arc::new(AppState {
        redis,
        jwt_secret: config.jwt_secret,
    });

    let app = walkietalk_auth::build_app(state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("auth service listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .await
        .expect("server error");
}
