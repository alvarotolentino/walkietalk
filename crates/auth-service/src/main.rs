use std::sync::Arc;

use sqlx::postgres::PgPool;
use tower_http::trace::TraceLayer;

use walkietalk_auth::config::Config;
use walkietalk_auth::state::AppState;

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
