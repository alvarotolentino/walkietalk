use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::task::JoinHandle;
use walkietalk_shared::db::{self, RedisConn};
use walkietalk_shared::error::AppError;
use walkietalk_shared::ids::{RoomId, UserId};

/// Holds in-memory state for a user currently holding the floor in a room.
struct FloorHolder {
    user_id: UserId,
    #[allow(dead_code)] // REASON: kept for diagnostics logging
    acquired_at: Instant,
    timeout_task: JoinHandle<()>,
}

/// Manages floor locks via Redis `SET NX EX 60` with an in-memory fast-path cache.
///
/// The distributed lock lives in Redis (`floor:{room_id}`), while a local
/// `DashMap` provides zero-cost holder checks for audio relay hot paths.
pub struct FloorManager {
    redis: RedisConn,
    floor_state: DashMap<RoomId, FloorHolder>,
}

impl FloorManager {
    /// Create a new FloorManager backed by the given Redis connection.
    pub fn new(redis: RedisConn) -> Self {
        Self {
            redis,
            floor_state: DashMap::new(),
        }
    }

    /// Attempt to acquire the floor in a room.
    ///
    /// Returns `true` if the floor was acquired, `false` if already held.
    /// On success, spawns a 60-second timeout task that calls `on_timeout`.
    pub async fn try_acquire<F>(
        &self,
        room_id: RoomId,
        _lock_key: i64,
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

        // Try the distributed Redis lock: SET floor:{room_id} {user_id} NX EX 60
        let acquired = db::try_acquire_floor(
            &mut self.redis.clone(),
            room_id.0,
            user_id.0,
        )
        .await?;

        if !acquired {
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
            },
        );

        Ok(true)
    }

    /// Check if a specific user holds the floor in a room (in-memory, zero network cost).
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

    /// Release the floor and return the holder's user_id.
    /// Removes from in-memory cache and deletes the Redis key.
    pub fn force_release(&self, room_id: &RoomId) -> Option<UserId> {
        self.floor_state.remove(room_id).map(|(rid, holder)| {
            holder.timeout_task.abort();
            // Fire-and-forget Redis key deletion
            let mut conn = self.redis.clone();
            tokio::spawn(async move {
                let _ = db::force_release_floor(&mut conn, rid.0).await;
            });
            holder.user_id
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    fn room_id() -> RoomId {
        RoomId(Uuid::new_v4())
    }

    fn user_id() -> UserId {
        UserId(Uuid::new_v4())
    }

    /// Connect to a local Redis/LuxDB instance for testing.
    async fn test_redis() -> RedisConn {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        db::connect(&url).await.expect("test redis connection")
    }

    #[tokio::test]
    async fn acquire_and_release_cycle() {
        let redis = test_redis().await;
        let mgr = FloorManager::new(redis);

        let rid = room_id();
        let uid = user_id();

        let acquired = mgr.try_acquire(rid, 0, uid, || {}).await.expect("acquire");
        assert!(acquired, "first acquire should succeed");
        assert!(mgr.is_held_by(&rid, &uid));
        assert_eq!(mgr.get_holder(&rid), Some(uid));

        let released = mgr.force_release(&rid);
        assert_eq!(released, Some(uid));

        assert!(!mgr.is_held_by(&rid, &uid));
        assert_eq!(mgr.get_holder(&rid), None);
    }

    #[tokio::test]
    async fn acquire_while_held_is_denied() {
        let redis = test_redis().await;
        let mgr = FloorManager::new(redis);

        let rid = room_id();
        let uid1 = user_id();
        let uid2 = user_id();

        let acquired = mgr.try_acquire(rid, 0, uid1, || {}).await.expect("acquire 1");
        assert!(acquired);

        let denied = mgr.try_acquire(rid, 0, uid2, || {}).await.expect("acquire 2");
        assert!(!denied, "second acquire should be denied");

        assert!(mgr.is_held_by(&rid, &uid1));
        assert!(!mgr.is_held_by(&rid, &uid2));

        mgr.force_release(&rid);
    }

    #[tokio::test]
    async fn timeout_fires_after_60_seconds() {
        let redis = test_redis().await;
        let mgr = FloorManager::new(redis);

        let rid = room_id();
        let uid = user_id();

        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();

        let acquired = mgr
            .try_acquire(rid, 0, uid, move || {
                timed_out_clone.store(true, Ordering::SeqCst);
            })
            .await
            .expect("acquire");
        assert!(acquired);

        tokio::task::yield_now().await;
        tokio::time::pause();

        tokio::time::advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        assert!(!timed_out.load(Ordering::SeqCst), "timeout should not fire before 60s");

        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(timed_out.load(Ordering::SeqCst), "timeout should fire after 60s");

        tokio::time::resume();
        mgr.force_release(&rid);
    }

    #[tokio::test]
    async fn release_aborts_timeout() {
        let redis = test_redis().await;
        let mgr = FloorManager::new(redis);

        let rid = room_id();
        let uid = user_id();

        let timed_out = Arc::new(AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();

        let acquired = mgr
            .try_acquire(rid, 0, uid, move || {
                timed_out_clone.store(true, Ordering::SeqCst);
            })
            .await
            .expect("acquire");
        assert!(acquired);

        mgr.force_release(&rid);

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(120)).await;
        tokio::task::yield_now().await;
        assert!(
            !timed_out.load(Ordering::SeqCst),
            "timeout should not fire after release aborts it"
        );
    }
}
