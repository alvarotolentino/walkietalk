use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;

/// Lightweight lock-free metrics for the signaling service.
///
/// Every field is an [`AtomicU64`] so recording is wait-free from any
/// async task or OS thread.  The `/metrics` endpoint snapshots everything
/// into [`MetricsSnapshot`] for JSON serialisation.
pub struct Metrics {
    start: Instant,

    // ── WebSocket connections ────────────────────────────────────────
    pub ws_connections_opened: AtomicU64,
    pub ws_connections_closed: AtomicU64,

    // ── Messages ────────────────────────────────────────────────────
    pub ws_text_messages_received: AtomicU64,
    pub ws_text_messages_sent: AtomicU64,

    // ── Audio ───────────────────────────────────────────────────────
    pub audio_frames_relayed: AtomicU64,
    pub audio_bytes_relayed: AtomicU64,

    // ── Floor ───────────────────────────────────────────────────────
    pub floor_requests: AtomicU64,
    pub floor_grants: AtomicU64,
    pub floor_denials: AtomicU64,
    pub floor_releases: AtomicU64,
    pub floor_timeouts: AtomicU64,

    // ── Room joins/leaves ───────────────────────────────────────────
    pub room_joins: AtomicU64,
    pub room_leaves: AtomicU64,

    // ── ZMQ ─────────────────────────────────────────────────────────
    pub zmq_frames_published: AtomicU64,
    pub zmq_frames_received: AtomicU64,
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
}
