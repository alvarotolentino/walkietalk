use std::time::{Duration, Instant};

use dashmap::DashMap;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres};
use tokio::task::JoinHandle;
use walkietalk_shared::error::AppError;
use walkietalk_shared::ids::{RoomId, UserId};

/// Holds in-memory state for a user currently holding the floor in a room.
struct FloorHolder {
    user_id: UserId,
    #[allow(dead_code)] // REASON: kept for diagnostics logging
    acquired_at: Instant,
    timeout_task: JoinHandle<()>,
    /// The connection on which the advisory lock is held.
    /// Dropping this returns the connection to the pool (and releases the lock).
    #[allow(dead_code)] // REASON: held to keep PostgreSQL advisory lock alive; read via Drop
    lock_conn: sqlx::pool::PoolConnection<Postgres>,
}

/// Manages floor locks via PostgreSQL advisory locks with an in-memory fast-path cache.
///
/// Uses a **dedicated** connection pool (`lock_pool`) separate from the query pool.
/// Each held floor keeps one connection checked out (holding the advisory lock).
pub struct FloorManager {
    lock_pool: PgPool,
    floor_state: DashMap<RoomId, FloorHolder>,
}

impl FloorManager {
    /// Create a new FloorManager with its own dedicated PgPool.
    pub async fn new(database_url: &str, max_connections: u32) -> Result<Self, AppError> {
        let lock_pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await
            .map_err(|e| AppError::Internal(format!("floor lock pool connect error: {e}")))?;

        Ok(Self {
            lock_pool,
            floor_state: DashMap::new(),
        })
    }

    /// Attempt to acquire the floor in a room.
    ///
    /// Returns `true` if the floor was acquired, `false` if already held.
    /// On success, spawns a 60-second timeout task that calls `on_timeout`.
    pub async fn try_acquire<F>(
        &self,
        room_id: RoomId,
        lock_key: i64,
        user_id: UserId,
        on_timeout: F,
    ) -> Result<bool, AppError>
    where
        F: FnOnce() + Send + 'static,
    {
        // Fast path: check in-memory state
        if self.floor_state.contains_key(&room_id) {
            return Ok(false);
        }

        // Acquire a dedicated connection for the advisory lock
        let mut conn = self.lock_pool.acquire().await.map_err(|e| {
            tracing::warn!("floor lock pool exhausted: {e}");
            AppError::Internal("floor lock pool exhausted".into())
        })?;

        // Try the PostgreSQL advisory lock (non-blocking)
        let locked: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock($1::bigint)")
                .bind(lock_key)
                .fetch_one(&mut *conn)
                .await
                .map_err(|e| AppError::Internal(format!("advisory lock error: {e}")))?;

        if !locked {
            return Ok(false);
        }

        // Spawn timeout task (60 seconds)
        let timeout_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            on_timeout();
        });

        self.floor_state.insert(
            room_id,
            FloorHolder {
                user_id,
                acquired_at: Instant::now(),
                timeout_task,
                lock_conn: conn,
            },
        );

        Ok(true)
    }

    /// Check if a specific user holds the floor in a room (in-memory, zero DB cost).
    pub fn is_held_by(&self, room_id: &RoomId, user_id: &UserId) -> bool {
        self.floor_state
            .get(room_id)
            .map(|h| h.user_id == *user_id)
            .unwrap_or(false)
    }

    /// Get the current floor holder in a room, if any.
    pub fn get_holder(&self, room_id: &RoomId) -> Option<UserId> {
        self.floor_state.get(room_id).map(|h| h.user_id)
    }

    /// Release the floor and return the holder's user_id. Simplified version that
    /// aborts the timeout and relies on connection drop to release the PG lock.
    pub fn force_release(&self, room_id: &RoomId) -> Option<UserId> {
        self.floor_state.remove(room_id).map(|(_, holder)| {
            holder.timeout_task.abort();
            // lock_conn drops here → connection returns to pool → advisory lock released
            holder.user_id
        })
    }

}

/// Look up a room's advisory lock key from the database.
pub async fn get_room_lock_key(db: &PgPool, room_id: &RoomId) -> Result<i64, AppError> {
    let key: i64 = sqlx::query_scalar("SELECT lock_key FROM rooms WHERE id = $1")
        .bind(room_id.0)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Internal(format!("db error: {e}")))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;
    Ok(key)
}
