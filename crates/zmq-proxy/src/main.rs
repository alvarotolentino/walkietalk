use std::sync::atomic::Ordering;

use zeromq::{PullSocket, Socket, SocketRecv, ZmqMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,walkietalk_zmq_proxy=debug"
                    .parse()
                    .expect("valid filter")
            }),
        )
        .init();

    let pull_addr = std::env::var("ZMQ_PULL_ADDR").unwrap_or_else(|_| "tcp://0.0.0.0:5559".into());
    let pub_addr = std::env::var("ZMQ_PUB_ADDR").unwrap_or_else(|_| "tcp://0.0.0.0:5560".into());

    let stats = walkietalk_zmq_proxy::run_proxy(&pull_addr, &pub_addr).await?;

    // Block on a dummy PULL recv that never completes (keeps main alive).
    // The actual proxy loop runs in a spawned task inside run_proxy.
    let mut sentinel = PullSocket::new();
    sentinel.bind("tcp://127.0.0.1:0").await?;
    let _: ZmqMessage = sentinel.recv().await?;

    // Unreachable, but if we ever get here, log final stats.
    tracing::info!(
        total_forwarded = stats.frames_forwarded.load(Ordering::Relaxed),
        "proxy shutting down"
    );
    Ok(())
}
