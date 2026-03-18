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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use testcontainers::{runners::AsyncRunner, ImageExt};
    use uuid::Uuid;

    /// Spin up a Postgres 16 container and return the database URL and container handle.
    async fn start_postgres() -> (
        String,
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
    ) {
        let image = testcontainers::GenericImage::new("postgres", "16-alpine")
            .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
            .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_env_var("POSTGRES_DB", "walkietalk_test");

        let container = image.start().await.expect("failed to start postgres container");

        let host_port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("failed to get mapped port");

        let host = container
            .get_host()
            .await
            .expect("failed to get container host");

        let db_url = format!("postgres://test:test@{host}:{host_port}/walkietalk_test");

        // Wait until Postgres actually accepts connections
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match PgPool::connect(&db_url).await {
                    Ok(p) => {
                        p.close().await;
                        break;
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        })
        .await
        .expect("timed out waiting for postgres");

        (db_url, container)
    }

    fn room_id() -> RoomId {
        RoomId(Uuid::new_v4())
    }

    fn user_id() -> UserId {
        UserId(Uuid::new_v4())
    }

    // Advisory lock keys can be any i64; we use simple incrementing values.
    // Each test uses a distinct key to avoid cross-test interference.

    #[tokio::test]
    async fn acquire_and_release_cycle() {
        let (db_url, _container) = start_postgres().await;
        let mgr = FloorManager::new(&db_url, 5).await.expect("floor manager");

        let rid = room_id();
        let uid = user_id();
        let lock_key = 1001_i64;

        // Acquire should succeed
        let acquired = mgr.try_acquire(rid, lock_key, uid, || {}).await.expect("acquire");
        assert!(acquired, "first acquire should succeed");
        assert!(mgr.is_held_by(&rid, &uid));
        assert_eq!(mgr.get_holder(&rid), Some(uid));

        // Release returns the holder
        let released = mgr.force_release(&rid);
        assert_eq!(released, Some(uid));

        // After release, floor is free
        assert!(!mgr.is_held_by(&rid, &uid));
        assert_eq!(mgr.get_holder(&rid), None);
    }

    #[tokio::test]
    async fn acquire_while_held_is_denied() {
        let (db_url, _container) = start_postgres().await;
        let mgr = FloorManager::new(&db_url, 5).await.expect("floor manager");

        let rid = room_id();
        let uid1 = user_id();
        let uid2 = user_id();
        let lock_key = 2001_i64;

        // First user acquires
        let acquired = mgr.try_acquire(rid, lock_key, uid1, || {}).await.expect("acquire 1");
        assert!(acquired);

        // Second user tries to acquire the same room → denied (in-memory fast path)
        let denied = mgr.try_acquire(rid, lock_key, uid2, || {}).await.expect("acquire 2");
        assert!(!denied, "second acquire should be denied");

        // Holder is still the first user
        assert!(mgr.is_held_by(&rid, &uid1));
        assert!(!mgr.is_held_by(&rid, &uid2));

        mgr.force_release(&rid);
    }

    #[tokio::test]
    async fn timeout_fires_after_60_seconds() {
        let (db_url, _container) = start_postgres().await;
        let mgr = FloorManager::new(&db_url, 5).await.expect("floor manager");

        let rid = room_id();
        let uid = user_id();
        let lock_key = 3001_i64;

        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();

        // Acquire BEFORE pausing time — needs real DB roundtrip
        let acquired = mgr
            .try_acquire(rid, lock_key, uid, move || {
                timed_out_clone.store(true, Ordering::SeqCst);
            })
            .await
            .expect("acquire");
        assert!(acquired);

        // Let the spawned timeout task start and register its sleep timer
        tokio::task::yield_now().await;

        // Now pause time so we can control the timeout task
        tokio::time::pause();

        // Advance 59 seconds — timeout should NOT have fired
        tokio::time::advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        assert!(!timed_out.load(Ordering::SeqCst), "timeout should not fire before 60s");

        // Advance past 60 seconds — timeout fires
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(timed_out.load(Ordering::SeqCst), "timeout should fire after 60s");

        // Clean up in-memory state (timeout callback doesn't remove it)
        tokio::time::resume();
        mgr.force_release(&rid);
    }

    #[tokio::test]
    async fn release_aborts_timeout() {
        let (db_url, _container) = start_postgres().await;
        let mgr = FloorManager::new(&db_url, 5).await.expect("floor manager");

        let rid = room_id();
        let uid = user_id();
        let lock_key = 4001_i64;

        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();

        // Acquire BEFORE pausing time — needs real DB roundtrip
        let acquired = mgr
            .try_acquire(rid, lock_key, uid, move || {
                timed_out_clone.store(true, Ordering::SeqCst);
            })
            .await
            .expect("acquire");
        assert!(acquired);

        // Release before timeout (still real time)
        mgr.force_release(&rid);

        // Now pause time and advance past 60s — callback should NOT fire (aborted)
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(120)).await;
        tokio::task::yield_now().await;
        assert!(
            !timed_out.load(Ordering::SeqCst),
            "timeout should not fire after release aborts it"
        );
    }

    #[tokio::test]
    async fn drop_releases_advisory_lock() {
        let (db_url, _container) = start_postgres().await;

        let rid = room_id();
        let uid = user_id();
        let lock_key = 5001_i64;

        // Scope the FloorManager so it drops (simulating disconnect)
        {
            let mgr = FloorManager::new(&db_url, 5).await.expect("floor manager");
            let acquired = mgr.try_acquire(rid, lock_key, uid, || {}).await.expect("acquire");
            assert!(acquired);
            // mgr drops here → FloorHolder drops → lock_conn drops → PG advisory lock released
        }

        // Create a fresh manager and verify the same lock_key can be acquired
        let mgr2 = FloorManager::new(&db_url, 5).await.expect("floor manager 2");
        let uid2 = user_id();
        let reacquired = mgr2
            .try_acquire(rid, lock_key, uid2, || {})
            .await
            .expect("re-acquire");
        assert!(
            reacquired,
            "advisory lock should be free after FloorManager drop (disconnect)"
        );

        mgr2.force_release(&rid);
    }
}
