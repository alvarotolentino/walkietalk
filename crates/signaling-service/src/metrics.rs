// ── Feature-gated metrics ───────────────────────────────────────────────
//
// All counter increments compile to **zero instructions** when the `metrics`
// feature is disabled. When enabled, every field is an [`AtomicU64`] so
// recording is wait-free from any async task or OS thread.
//
// Use the `record!` macro at call sites:
//   `record!(state.metrics, audio_frames_relayed);`
//   `record!(state.metrics, audio_bytes_relayed, data.len() as u64);`

/// Convenience macro for incrementing a metric counter.
///
/// When the `metrics` feature is **enabled**, this expands to a
/// `fetch_add(1 | val, Ordering::Relaxed)` call.
/// When **disabled**, it expands to nothing — zero runtime cost.
#[cfg(feature = "metrics")]
#[macro_export]
macro_rules! record {
    ($metrics:expr, $field:ident) => {
        $metrics
            .$field
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    };
    ($metrics:expr, $field:ident, $val:expr) => {
        $metrics
            .$field
            .fetch_add($val, std::sync::atomic::Ordering::Relaxed)
    };
}

#[cfg(not(feature = "metrics"))]
#[macro_export]
macro_rules! record {
    ($metrics:expr, $field:ident) => {
        /* no-op */
    };
    ($metrics:expr, $field:ident, $val:expr) => {
        /* no-op */
    };
}

// ── Full metrics (feature = "metrics") ──────────────────────────────────

#[cfg(feature = "metrics")]
mod inner {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use serde::Serialize;

    /// Lightweight lock-free metrics for the signaling service.
    pub struct Metrics {
        start: Instant,

        // ── WebSocket connections ────────────────────────────────────
        pub ws_connections_opened: AtomicU64,
        pub ws_connections_closed: AtomicU64,

        // ── Messages ────────────────────────────────────────────────
        pub ws_text_messages_received: AtomicU64,
        pub ws_text_messages_sent: AtomicU64,

        // ── Audio ───────────────────────────────────────────────────
        pub audio_frames_relayed: AtomicU64,
        pub audio_bytes_relayed: AtomicU64,

        // ── Floor ───────────────────────────────────────────────────
        pub floor_requests: AtomicU64,
        pub floor_grants: AtomicU64,
        pub floor_denials: AtomicU64,
        pub floor_releases: AtomicU64,
        pub floor_timeouts: AtomicU64,

        // ── Room joins/leaves ───────────────────────────────────────
        pub room_joins: AtomicU64,
        pub room_leaves: AtomicU64,

        // ── ZMQ ─────────────────────────────────────────────────────
        pub zmq_frames_published: AtomicU64,
        pub zmq_frames_received: AtomicU64,

        // ── Throughput (computed per snapshot interval) ──────────────
        pub ws_binary_frames_received: AtomicU64,
        pub ws_binary_bytes_received: AtomicU64,
        pub ws_text_bytes_sent: AtomicU64,
        pub ws_binary_bytes_sent: AtomicU64,
        pub redis_commands_issued: AtomicU64,
    }

    impl Metrics {
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
                ws_connections_opened: AtomicU64::new(0),
                ws_connections_closed: AtomicU64::new(0),
                ws_text_messages_received: AtomicU64::new(0),
                ws_text_messages_sent: AtomicU64::new(0),
                audio_frames_relayed: AtomicU64::new(0),
                audio_bytes_relayed: AtomicU64::new(0),
                floor_requests: AtomicU64::new(0),
                floor_grants: AtomicU64::new(0),
                floor_denials: AtomicU64::new(0),
                floor_releases: AtomicU64::new(0),
                floor_timeouts: AtomicU64::new(0),
                room_joins: AtomicU64::new(0),
                room_leaves: AtomicU64::new(0),
                zmq_frames_published: AtomicU64::new(0),
                zmq_frames_received: AtomicU64::new(0),
                ws_binary_frames_received: AtomicU64::new(0),
                ws_binary_bytes_received: AtomicU64::new(0),
                ws_text_bytes_sent: AtomicU64::new(0),
                ws_binary_bytes_sent: AtomicU64::new(0),
                redis_commands_issued: AtomicU64::new(0),
            }
        }

        pub fn snapshot(&self) -> MetricsSnapshot {
            let uptime = self.start.elapsed();
            let opened = self.ws_connections_opened.load(Ordering::Relaxed);
            let closed = self.ws_connections_closed.load(Ordering::Relaxed);

            MetricsSnapshot {
                uptime_secs: uptime.as_secs_f64(),
                ws_connections_active: opened.saturating_sub(closed),
                ws_connections_opened: opened,
                ws_connections_closed: closed,
                ws_text_messages_received: self.ws_text_messages_received.load(Ordering::Relaxed),
                ws_text_messages_sent: self.ws_text_messages_sent.load(Ordering::Relaxed),
                audio_frames_relayed: self.audio_frames_relayed.load(Ordering::Relaxed),
                audio_bytes_relayed: self.audio_bytes_relayed.load(Ordering::Relaxed),
                floor_requests: self.floor_requests.load(Ordering::Relaxed),
                floor_grants: self.floor_grants.load(Ordering::Relaxed),
                floor_denials: self.floor_denials.load(Ordering::Relaxed),
                floor_releases: self.floor_releases.load(Ordering::Relaxed),
                floor_timeouts: self.floor_timeouts.load(Ordering::Relaxed),
                room_joins: self.room_joins.load(Ordering::Relaxed),
                room_leaves: self.room_leaves.load(Ordering::Relaxed),
                zmq_frames_published: self.zmq_frames_published.load(Ordering::Relaxed),
                zmq_frames_received: self.zmq_frames_received.load(Ordering::Relaxed),
                ws_binary_frames_received: self.ws_binary_frames_received.load(Ordering::Relaxed),
                ws_binary_bytes_received: self.ws_binary_bytes_received.load(Ordering::Relaxed),
                ws_text_bytes_sent: self.ws_text_bytes_sent.load(Ordering::Relaxed),
                ws_binary_bytes_sent: self.ws_binary_bytes_sent.load(Ordering::Relaxed),
                redis_commands_issued: self.redis_commands_issued.load(Ordering::Relaxed),
            }
        }
    }

    #[derive(Serialize)]
    pub struct MetricsSnapshot {
        pub uptime_secs: f64,
        pub ws_connections_active: u64,
        pub ws_connections_opened: u64,
        pub ws_connections_closed: u64,
        pub ws_text_messages_received: u64,
        pub ws_text_messages_sent: u64,
        pub audio_frames_relayed: u64,
        pub audio_bytes_relayed: u64,
        pub floor_requests: u64,
        pub floor_grants: u64,
        pub floor_denials: u64,
        pub floor_releases: u64,
        pub floor_timeouts: u64,
        pub room_joins: u64,
        pub room_leaves: u64,
        pub zmq_frames_published: u64,
        pub zmq_frames_received: u64,
        // ── Throughput counters ──────────────────────────────────────
        pub ws_binary_frames_received: u64,
        pub ws_binary_bytes_received: u64,
        pub ws_text_bytes_sent: u64,
        pub ws_binary_bytes_sent: u64,
        pub redis_commands_issued: u64,
    }
}

// ── Stub metrics (no-op when feature disabled) ──────────────────────────

#[cfg(not(feature = "metrics"))]
mod inner {
    use serde::Serialize;

    /// Zero-cost stub — all methods are no-ops.
    pub struct Metrics;

    impl Metrics {
        pub fn new() -> Self {
            Self
        }

        pub fn snapshot(&self) -> MetricsSnapshot {
            MetricsSnapshot {}
        }
    }

    /// Empty snapshot when metrics are disabled.
    #[derive(Serialize)]
    pub struct MetricsSnapshot {}
}

pub use inner::{Metrics, MetricsSnapshot};
