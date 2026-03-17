use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use zeromq::{PubSocket, PullSocket, Socket, SocketRecv, SocketSend};

/// Frame counters for periodic throughput logging.
pub struct Stats {
    pub frames_forwarded: AtomicU64,
}

/// Stateless PULL/PUB fan-out proxy.
///
/// Signaling nodes push frames (with a topic prefix) to the PULL socket.
/// The proxy forwards them verbatim to the PUB socket, which fans out to
/// all connected SUB sockets. Topic filtering is handled by ZeroMQ at the
/// subscriber level.
///
/// Adapted from the spec's XSUB/XPUB pattern because the `zeromq` crate
/// does not implement XSubSocket.
pub async fn run_proxy(pull_addr: &str, pub_addr: &str) -> Result<Arc<Stats>, Box<dyn std::error::Error>> {
    let mut pull = PullSocket::new();
    pull.bind(pull_addr).await?;
    tracing::info!("PULL bound on {pull_addr} (publishers connect here)");

    let mut publisher = PubSocket::new();
    publisher.bind(pub_addr).await?;
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

    let stats_ret = Arc::clone(&stats);

    // Proxy loop: receive from PULL, forward to PUB
    tokio::spawn(async move {
        loop {
            match pull.recv().await {
                Ok(msg) => {
                    stats.frames_forwarded.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(frames = msg.len(), "PULL → PUB");
                    if let Err(e) = publisher.send(msg).await {
                        tracing::error!("PUB send error: {e}");
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("PULL recv error: {e}");
                    break;
                }
            }
        }
    });

    Ok(stats_ret)
}
