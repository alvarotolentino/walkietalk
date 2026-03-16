use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use zeromq::{PubSocket, PullSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// Frame counters for periodic throughput logging.
struct Stats {
    frames_forwarded: AtomicU64,
}

/// Stateless PULL/PUB fan-out proxy.
///
/// Signaling nodes push frames (with a topic prefix) to the PULL socket.
/// The proxy forwards them verbatim to the PUB socket, which fans out to
/// all connected SUB sockets. Topic filtering is handled by ZeroMQ at the
/// subscriber level.
///
/// Adapted from the spec's XSUB/XPUB pattern because the `zeromq` crate
/// (0.6.0-pre.1) does not implement XSubSocket.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,walkietalk_zmq_proxy=debug".parse().expect("valid filter")),
        )
        .init();

    let pull_addr = std::env::var("ZMQ_PULL_ADDR").unwrap_or_else(|_| "tcp://0.0.0.0:5559".into());
    let pub_addr = std::env::var("ZMQ_PUB_ADDR").unwrap_or_else(|_| "tcp://0.0.0.0:5560".into());

    let mut pull = PullSocket::new();
    pull.bind(&pull_addr).await?;
    tracing::info!("PULL bound on {pull_addr} (publishers connect here)");

    let mut publisher = PubSocket::new();
    publisher.bind(&pub_addr).await?;
    tracing::info!("PUB bound on {pub_addr} (subscribers connect here)");

    let stats = Arc::new(Stats {
        frames_forwarded: AtomicU64::new(0),
    });

    // Periodically log throughput counters every 10 seconds
    let stats_clone = Arc::clone(&stats);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let fwd = stats_clone.frames_forwarded.swap(0, Ordering::Relaxed);
            if fwd > 0 {
                tracing::info!(frames_forwarded = fwd, "throughput (last 10s)");
            }
        }
    });

    tracing::info!("ZMQ proxy running — forwarding PULL → PUB");

    // Proxy loop: receive from PULL, forward to PUB
    loop {
        let msg: ZmqMessage = pull.recv().await?;
        stats.frames_forwarded.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(frames = msg.len(), "PULL → PUB");
        publisher.send(msg).await?;
    }
}
