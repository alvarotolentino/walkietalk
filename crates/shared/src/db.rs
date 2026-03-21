//! Redis/LuxDB data-access layer.
//!
//! Provides typed helpers for all entity CRUD operations formerly backed by PostgreSQL.
//! Each entity is modelled as a Redis Hash keyed by UUID. Secondary indexes use
//! Sets, Sorted Sets, and plain string keys for lookups.
//!
//! # Key Schema
//!
//! ## Users
//! - `user:{uuid}`                  → Hash { username, email, password_hash, display_name, avatar_url, created_at, updated_at }
//! - `user:email:{email}`           → String (user_id)
//! - `user:username:{username}`     → String (user_id)
//!
//! ## Devices
//! - `device:{uuid}`               → Hash { user_id, name, platform, push_token, last_seen, created_at }
//! - `user:{uuid}:devices`          → Set of device UUIDs
//!
//! ## Rooms
//! - `room:{uuid}`                  → Hash { slug, name, description, owner_id, visibility, invite_code, lock_key, created_at, updated_at }
//! - `room:slug:{slug}`             → String (room_id)
//! - `room:invite:{code}`           → String (room_id)
//! - `rooms:public`                 → Sorted Set (score = -created_at_ms, member = room_id)  — newest first
//! - `room:lock_key_seq`            → Counter (INCR, replaces PG IDENTITY)
//!
//! ## Room Members
//! - `room:{uuid}:members`          → Set of user_ids
//! - `room:{uuid}:member:{user_id}` → Hash { role, joined_at }
//! - `user:{uuid}:rooms`            → Set of room_ids
//!
//! ## Refresh Tokens  
//! - `refresh:{token_hash}`         → Hash { id, user_id, device_id, expires_at, revoked }
//! - `user:{uuid}:refresh_tokens`   → Set of token_hashes
//!
//! ## Floor Locks
//! - `floor:{room_id}`              → String (user_id) with NX + EX 60

use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use uuid::Uuid;

use crate::error::AppError;

/// Type alias for our async Redis connection (auto-reconnecting).
pub type RedisConn = ConnectionManager;

/// Connect to Redis/LuxDB and return a connection manager.
pub async fn connect(redis_url: &str) -> Result<RedisConn, AppError> {
    let client = redis::Client::open(redis_url)
        .map_err(|e| AppError::Internal(format!("redis client error: {e}")))?;
    let conn = ConnectionManager::new(client)
        .await
        .map_err(|e| AppError::Internal(format!("redis connect error: {e}")))?;
    Ok(conn)
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn ts_to_string(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn string_to_ts(s: &str) -> Result<DateTime<Utc>, AppError> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| AppError::Internal(format!("timestamp parse error: {e}")))
}

fn opt_field(map: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    map.get(key).filter(|v| !v.is_empty()).cloned()
}

fn req_field<'a>(
    map: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Result<&'a str, AppError> {
    map.get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| AppError::Internal(format!("missing field: {key}")))
}

fn uuid_field(map: &std::collections::HashMap<String, String>, key: &str) -> Result<Uuid, AppError> {
    let s = req_field(map, key)?;
    s.parse::<Uuid>()
        .map_err(|e| AppError::Internal(format!("uuid parse ({key}): {e}")))
}

// ═══════════════════════════════════════════════════════════════════════════
// USERS
// ═══════════════════════════════════════════════════════════════════════════

/// User record deserialized from Redis hash.
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new user. Returns `Err(Conflict)` if username or email already exists.
pub async fn create_user(
    conn: &mut RedisConn,
    username: &str,
    email: &str,
    password_hash: &str,
    display_name: &str,
) -> Result<UserRecord, AppError> {
    // Check uniqueness
    let existing_email: Option<String> = conn
        .get(format!("user:email:{email}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if existing_email.is_some() {
        return Err(AppError::Conflict(
            "user with this email or username already exists".into(),
        ));
    }
    let existing_username: Option<String> = conn
        .get(format!("user:username:{username}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if existing_username.is_some() {
        return Err(AppError::Conflict(
            "user with this email or username already exists".into(),
        ));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = ts_to_string(&now);
    let key = format!("user:{id}");

    redis::pipe()
        .atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("username").arg(username)
        .arg("email").arg(email)
        .arg("password_hash").arg(password_hash)
        .arg("display_name").arg(display_name)
        .arg("avatar_url").arg("")
        .arg("created_at").arg(&now_str)
        .arg("updated_at").arg(&now_str)
        .ignore()
        .cmd("SET").arg(format!("user:email:{email}")).arg(id.to_string()).ignore()
        .cmd("SET").arg(format!("user:username:{username}")).arg(id.to_string()).ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(UserRecord {
        id,
        username: username.to_string(),
        email: email.to_string(),
        password_hash: password_hash.to_string(),
        display_name: display_name.to_string(),
        avatar_url: None,
        created_at: now,
        updated_at: now,
    })
}

/// Get a user by ID.
pub async fn get_user(conn: &mut RedisConn, id: Uuid) -> Result<Option<UserRecord>, AppError> {
    let map: std::collections::HashMap<String, String> = conn
        .hgetall(format!("user:{id}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if map.is_empty() {
        return Ok(None);
    }
    Ok(Some(UserRecord {
        id,
        username: req_field(&map, "username")?.to_string(),
        email: req_field(&map, "email")?.to_string(),
        password_hash: req_field(&map, "password_hash")?.to_string(),
        display_name: req_field(&map, "display_name")?.to_string(),
        avatar_url: opt_field(&map, "avatar_url"),
        created_at: string_to_ts(req_field(&map, "created_at")?)?,
        updated_at: string_to_ts(req_field(&map, "updated_at")?)?,
    }))
}

/// Get a user by email.
pub async fn get_user_by_email(
    conn: &mut RedisConn,
    email: &str,
) -> Result<Option<UserRecord>, AppError> {
    let id_str: Option<String> = conn
        .get(format!("user:email:{email}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    match id_str {
        Some(s) => {
            let id: Uuid = s
                .parse()
                .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
            get_user(conn, id).await
        }
        None => Ok(None),
    }
}

/// Get just the display_name for a user.
pub async fn get_display_name(
    conn: &mut RedisConn,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    let name: Option<String> = conn
        .hget(format!("user:{user_id}"), "display_name")
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(name)
}

// ═══════════════════════════════════════════════════════════════════════════
// DEVICES
// ═══════════════════════════════════════════════════════════════════════════

/// Device record deserialized from Redis hash.
#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub async fn create_device(
    conn: &mut RedisConn,
    user_id: Uuid,
    name: &str,
    platform: &str,
) -> Result<DeviceRecord, AppError> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = ts_to_string(&now);
    let key = format!("device:{id}");

    redis::pipe()
        .atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("user_id").arg(user_id.to_string())
        .arg("name").arg(name)
        .arg("platform").arg(platform)
        .arg("push_token").arg("")
        .arg("last_seen").arg("")
        .arg("created_at").arg(&now_str)
        .ignore()
        .cmd("SADD").arg(format!("user:{user_id}:devices")).arg(id.to_string()).ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(DeviceRecord {
        id,
        user_id,
        name: name.to_string(),
        platform: platform.to_string(),
        push_token: None,
        last_seen: None,
        created_at: now,
    })
}

pub async fn list_devices(
    conn: &mut RedisConn,
    user_id: Uuid,
) -> Result<Vec<DeviceRecord>, AppError> {
    let ids: Vec<String> = conn
        .smembers(format!("user:{user_id}:devices"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let mut devices = Vec::with_capacity(ids.len());
    for id_str in &ids {
        let id: Uuid = id_str
            .parse()
            .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
        let map: std::collections::HashMap<String, String> = conn
            .hgetall(format!("device:{id}"))
            .await
            .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
        if map.is_empty() {
            continue;
        }
        devices.push(DeviceRecord {
            id,
            user_id,
            name: req_field(&map, "name")?.to_string(),
            platform: req_field(&map, "platform")?.to_string(),
            push_token: opt_field(&map, "push_token"),
            last_seen: opt_field(&map, "last_seen")
                .map(|s| string_to_ts(&s))
                .transpose()?,
            created_at: string_to_ts(req_field(&map, "created_at")?)?,
        });
    }
    // Sort newest first
    devices.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(devices)
}

pub async fn delete_device(
    conn: &mut RedisConn,
    device_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    // Verify ownership
    let owner: Option<String> = conn
        .hget(format!("device:{device_id}"), "user_id")
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    match owner {
        Some(ref o) if o == &user_id.to_string() => {}
        _ => return Ok(false),
    }

    redis::pipe()
        .atomic()
        .cmd("DEL").arg(format!("device:{device_id}")).ignore()
        .cmd("SREM").arg(format!("user:{user_id}:devices")).arg(device_id.to_string()).ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(true)
}

// ═══════════════════════════════════════════════════════════════════════════
// REFRESH TOKENS
// ═══════════════════════════════════════════════════════════════════════════

/// Refresh token record.
#[derive(Debug, Clone)]
pub struct RefreshTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_id: Option<Uuid>,
}

/// Store a new refresh token hash with a 7-day TTL.
pub async fn create_refresh_token(
    conn: &mut RedisConn,
    user_id: Uuid,
    device_id: Option<Uuid>,
    token_hash: &str,
) -> Result<(), AppError> {
    let id = Uuid::new_v4();
    let key = format!("refresh:{token_hash}");
    let device_str = device_id.map(|d| d.to_string()).unwrap_or_default();
    let ttl_secs: u64 = 7 * 24 * 3600; // 7 days

    redis::pipe()
        .atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("id").arg(id.to_string())
        .arg("user_id").arg(user_id.to_string())
        .arg("device_id").arg(&device_str)
        .arg("revoked").arg("false")
        .ignore()
        .cmd("EXPIRE").arg(&key).arg(ttl_secs).ignore()
        .cmd("SADD").arg(format!("user:{user_id}:refresh_tokens")).arg(token_hash).ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(())
}

/// Validate and return a refresh token record. Returns None if not found, expired, or revoked.
pub async fn get_refresh_token(
    conn: &mut RedisConn,
    token_hash: &str,
) -> Result<Option<RefreshTokenRecord>, AppError> {
    let map: std::collections::HashMap<String, String> = conn
        .hgetall(format!("refresh:{token_hash}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if map.is_empty() {
        return Ok(None);
    }
    let revoked = req_field(&map, "revoked")? == "true";
    if revoked {
        return Ok(None);
    }
    let user_id = uuid_field(&map, "user_id")?;
    let device_id = opt_field(&map, "device_id")
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Uuid>())
        .transpose()
        .map_err(|e| AppError::Internal(format!("uuid parse (device_id): {e}")))?;
    let id = uuid_field(&map, "id")?;

    Ok(Some(RefreshTokenRecord {
        id,
        user_id,
        device_id,
    }))
}

/// Revoke a single refresh token.
pub async fn revoke_refresh_token(
    conn: &mut RedisConn,
    token_hash: &str,
) -> Result<(), AppError> {
    let _: () = conn
        .hset(format!("refresh:{token_hash}"), "revoked", "true")
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(())
}

/// Revoke all active refresh tokens for a user.
pub async fn revoke_all_refresh_tokens(
    conn: &mut RedisConn,
    user_id: Uuid,
) -> Result<(), AppError> {
    let hashes: Vec<String> = conn
        .smembers(format!("user:{user_id}:refresh_tokens"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    for hash in &hashes {
        let _: () = conn
            .hset(format!("refresh:{hash}"), "revoked", "true")
            .await
            .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// ROOMS
// ═══════════════════════════════════════════════════════════════════════════

/// Room record deserialized from Redis hash.
#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub id: Uuid,
    pub lock_key: i64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub visibility: String,
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new room and auto-assign a lock_key via INCR.
pub async fn create_room(
    conn: &mut RedisConn,
    name: &str,
    description: Option<&str>,
    slug: &str,
    owner_id: Uuid,
    visibility: &str,
) -> Result<RoomRecord, AppError> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = ts_to_string(&now);

    // Atomic lock_key sequence
    let lock_key: i64 = conn
        .incr("room:lock_key_seq", 1i64)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let key = format!("room:{id}");
    let desc = description.unwrap_or("");

    let score = -(now.timestamp_millis() as f64); // negative for newest-first ZRANGEBYSCORE

    let mut pipe = redis::pipe();
    pipe.atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("slug").arg(slug)
        .arg("name").arg(name)
        .arg("description").arg(desc)
        .arg("owner_id").arg(owner_id.to_string())
        .arg("visibility").arg(visibility)
        .arg("invite_code").arg("")
        .arg("lock_key").arg(lock_key)
        .arg("created_at").arg(&now_str)
        .arg("updated_at").arg(&now_str)
        .ignore()
        .cmd("SET").arg(format!("room:slug:{slug}")).arg(id.to_string()).ignore();

    if visibility == "public" {
        pipe.cmd("ZADD")
            .arg("rooms:public")
            .arg(score)
            .arg(id.to_string())
            .ignore();
    }

    pipe.exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(RoomRecord {
        id,
        lock_key,
        slug: slug.to_string(),
        name: name.to_string(),
        description: description.map(String::from),
        owner_id,
        visibility: visibility.to_string(),
        invite_code: None,
        created_at: now,
        updated_at: now,
    })
}

/// Get a room by ID.
pub async fn get_room(conn: &mut RedisConn, id: Uuid) -> Result<Option<RoomRecord>, AppError> {
    let map: std::collections::HashMap<String, String> = conn
        .hgetall(format!("room:{id}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if map.is_empty() {
        return Ok(None);
    }
    parse_room_record(id, &map)
}

fn parse_room_record(
    id: Uuid,
    map: &std::collections::HashMap<String, String>,
) -> Result<Option<RoomRecord>, AppError> {
    Ok(Some(RoomRecord {
        id,
        lock_key: req_field(map, "lock_key")?
            .parse()
            .map_err(|e| AppError::Internal(format!("lock_key parse: {e}")))?,
        slug: req_field(map, "slug")?.to_string(),
        name: req_field(map, "name")?.to_string(),
        description: opt_field(map, "description"),
        owner_id: uuid_field(map, "owner_id")?,
        visibility: req_field(map, "visibility")?.to_string(),
        invite_code: opt_field(map, "invite_code"),
        created_at: string_to_ts(req_field(map, "created_at")?)?,
        updated_at: string_to_ts(req_field(map, "updated_at")?)?,
    }))
}

/// Update mutable room fields.
pub async fn update_room(
    conn: &mut RedisConn,
    id: Uuid,
    name: &str,
    description: Option<&str>,
    visibility: &str,
    old_visibility: &str,
) -> Result<(), AppError> {
    let now_str = ts_to_string(&Utc::now());
    let key = format!("room:{id}");

    let mut pipe = redis::pipe();
    pipe.atomic()
        .cmd("HSET")
        .arg(&key)
        .arg("name").arg(name)
        .arg("description").arg(description.unwrap_or(""))
        .arg("visibility").arg(visibility)
        .arg("updated_at").arg(&now_str)
        .ignore();

    // Manage public room index
    if old_visibility != visibility {
        if visibility == "public" {
            let created_at_str: Option<String> = conn
                .hget(&key, "created_at")
                .await
                .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
            if let Some(s) = created_at_str {
                let dt = string_to_ts(&s)?;
                let score = -(dt.timestamp_millis() as f64);
                pipe.cmd("ZADD")
                    .arg("rooms:public")
                    .arg(score)
                    .arg(id.to_string())
                    .ignore();
            }
        } else {
            pipe.cmd("ZREM")
                .arg("rooms:public")
                .arg(id.to_string())
                .ignore();
        }
    }

    pipe.exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(())
}

/// Delete a room and all associated data (members, indexes).
pub async fn delete_room(conn: &mut RedisConn, room: &RoomRecord) -> Result<(), AppError> {
    // Get all members to clean up reverse indexes
    let member_ids: Vec<String> = conn
        .smembers(format!("room:{}:members", room.id))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let mut pipe = redis::pipe();
    pipe.atomic();

    // Remove member reverse indexes and member detail hashes
    for uid_str in &member_ids {
        pipe.cmd("SREM")
            .arg(format!("user:{uid_str}:rooms"))
            .arg(room.id.to_string())
            .ignore();
        pipe.cmd("DEL")
            .arg(format!("room:{}:member:{uid_str}", room.id))
            .ignore();
    }

    // Remove the room itself and all indexes
    pipe.cmd("DEL").arg(format!("room:{}", room.id)).ignore()
        .cmd("DEL").arg(format!("room:{}:members", room.id)).ignore()
        .cmd("DEL").arg(format!("room:slug:{}", room.slug)).ignore()
        .cmd("ZREM").arg("rooms:public").arg(room.id.to_string()).ignore();

    if let Some(ref code) = room.invite_code {
        pipe.cmd("DEL")
            .arg(format!("room:invite:{code}"))
            .ignore();
    }

    // Remove floor lock if held
    pipe.cmd("DEL")
        .arg(format!("floor:{}", room.id))
        .ignore();

    pipe.exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(())
}

/// Set a room's invite code (replaces old one if any).
pub async fn set_room_invite_code(
    conn: &mut RedisConn,
    room_id: Uuid,
    old_code: Option<&str>,
    new_code: &str,
) -> Result<(), AppError> {
    let mut pipe = redis::pipe();
    pipe.atomic();

    // Remove old invite index
    if let Some(old) = old_code {
        pipe.cmd("DEL").arg(format!("room:invite:{old}")).ignore();
    }

    pipe.cmd("HSET")
        .arg(format!("room:{room_id}"))
        .arg("invite_code").arg(new_code)
        .arg("updated_at").arg(ts_to_string(&Utc::now()))
        .ignore()
        .cmd("SET").arg(format!("room:invite:{new_code}")).arg(room_id.to_string()).ignore();

    pipe.exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(())
}

/// Get the lock_key for a room.
pub async fn get_room_lock_key(conn: &mut RedisConn, room_id: Uuid) -> Result<Option<i64>, AppError> {
    let val: Option<String> = conn
        .hget(format!("room:{room_id}"), "lock_key")
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    match val {
        Some(s) => {
            let key: i64 = s
                .parse()
                .map_err(|e| AppError::Internal(format!("lock_key parse: {e}")))?;
            Ok(Some(key))
        }
        None => Ok(None),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ROOM MEMBERS
// ═══════════════════════════════════════════════════════════════════════════

/// Add a member to a room.
pub async fn add_room_member(
    conn: &mut RedisConn,
    room_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    let now_str = ts_to_string(&Utc::now());
    redis::pipe()
        .atomic()
        .cmd("SADD").arg(format!("room:{room_id}:members")).arg(user_id.to_string()).ignore()
        .cmd("SADD").arg(format!("user:{user_id}:rooms")).arg(room_id.to_string()).ignore()
        .cmd("HSET")
        .arg(format!("room:{room_id}:member:{user_id}"))
        .arg("role").arg(role)
        .arg("joined_at").arg(&now_str)
        .ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(())
}

/// Check if a user is a member of a room.
pub async fn is_room_member(
    conn: &mut RedisConn,
    room_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let is_member: bool = conn
        .sismember(format!("room:{room_id}:members"), user_id.to_string())
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(is_member)
}

/// Get the member count for a room.
pub async fn room_member_count(conn: &mut RedisConn, room_id: Uuid) -> Result<i64, AppError> {
    let count: i64 = conn
        .scard(format!("room:{room_id}:members"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(count)
}

/// Remove a member from a room.
pub async fn remove_room_member(
    conn: &mut RedisConn,
    room_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let removed: i64 = conn
        .srem(format!("room:{room_id}:members"), user_id.to_string())
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    if removed == 0 {
        return Ok(false);
    }
    redis::pipe()
        .atomic()
        .cmd("SREM").arg(format!("user:{user_id}:rooms")).arg(room_id.to_string()).ignore()
        .cmd("DEL").arg(format!("room:{room_id}:member:{user_id}")).ignore()
        .exec_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(true)
}

/// Member info for ROOM_STATE messages.
#[derive(Debug, Clone)]
pub struct RoomMemberInfo {
    pub user_id: Uuid,
    pub display_name: String,
}

/// Get all members of a room with display names.
pub async fn get_room_member_info(
    conn: &mut RedisConn,
    room_id: Uuid,
) -> Result<Vec<RoomMemberInfo>, AppError> {
    let member_ids: Vec<String> = conn
        .smembers(format!("room:{room_id}:members"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let mut infos = Vec::with_capacity(member_ids.len());
    for uid_str in &member_ids {
        let uid: Uuid = uid_str
            .parse()
            .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
        let display_name: String = conn
            .hget(format!("user:{uid}"), "display_name")
            .await
            .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
        infos.push(RoomMemberInfo {
            user_id: uid,
            display_name,
        });
    }
    Ok(infos)
}

/// List all rooms a user belongs to (with room data and member count).
pub async fn list_user_rooms(
    conn: &mut RedisConn,
    user_id: Uuid,
) -> Result<Vec<(RoomRecord, i64)>, AppError> {
    let room_ids: Vec<String> = conn
        .smembers(format!("user:{user_id}:rooms"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let mut rooms = Vec::with_capacity(room_ids.len());
    for rid_str in &room_ids {
        let rid: Uuid = rid_str
            .parse()
            .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
        if let Some(room) = get_room(conn, rid).await? {
            let count = room_member_count(conn, rid).await?;
            rooms.push((room, count));
        }
    }
    // Sort newest first
    rooms.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));
    Ok(rooms)
}

/// List public rooms (newest first, with pagination and optional name filter).
pub async fn list_public_rooms(
    conn: &mut RedisConn,
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<(RoomRecord, i64)>, AppError> {
    // Get all public room IDs from sorted set (already sorted newest-first by negative score)
    let all_ids: Vec<String> = conn
        .zrangebyscore("rooms:public", "-inf", "+inf")
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    let mut result = Vec::new();
    for rid_str in &all_ids {
        let rid: Uuid = rid_str
            .parse()
            .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
        if let Some(room) = get_room(conn, rid).await? {
            // Name filter (case-insensitive contains)
            if let Some(pattern) = search {
                if !room.name.to_lowercase().contains(&pattern.to_lowercase()) {
                    continue;
                }
            }
            let count = room_member_count(conn, rid).await?;
            result.push((room, count));
        }
    }

    // Apply pagination
    let start = offset as usize;
    let end = (offset + limit) as usize;
    let page = if start < result.len() {
        result[start..end.min(result.len())].to_vec()
    } else {
        Vec::new()
    };

    Ok(page)
}

// ═══════════════════════════════════════════════════════════════════════════
// FLOOR LOCKS (SET NX EX pattern)
// ═══════════════════════════════════════════════════════════════════════════

/// Attempt to acquire the floor lock for a room.
/// Uses `SET floor:{room_id} {user_id} NX EX 60`.
/// Returns `true` if acquired, `false` if already held.
pub async fn try_acquire_floor(
    conn: &mut RedisConn,
    room_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let result: Option<String> = redis::cmd("SET")
        .arg(format!("floor:{room_id}"))
        .arg(user_id.to_string())
        .arg("NX")
        .arg("EX")
        .arg(60u64)
        .query_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;

    Ok(result.is_some()) // "OK" if set, None if already exists
}

/// Get the current floor holder for a room.
pub async fn get_floor_holder(
    conn: &mut RedisConn,
    room_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let holder: Option<String> = conn
        .get(format!("floor:{room_id}"))
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    match holder {
        Some(s) => {
            let uid: Uuid = s
                .parse()
                .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
            Ok(Some(uid))
        }
        None => Ok(None),
    }
}

/// Release the floor lock for a room (only if held by the expected user — atomic).
/// Uses a Lua script for atomic check-and-delete.
pub async fn release_floor(
    conn: &mut RedisConn,
    room_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let script = redis::Script::new(
        r#"
        if redis.call("GET", KEYS[1]) == ARGV[1] then
            return redis.call("DEL", KEYS[1])
        else
            return 0
        end
        "#,
    );
    let result: i64 = script
        .key(format!("floor:{room_id}"))
        .arg(user_id.to_string())
        .invoke_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    Ok(result == 1)
}

/// Force-release the floor lock regardless of holder.
pub async fn force_release_floor(
    conn: &mut RedisConn,
    room_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    // GET then DEL atomically via Lua
    let script = redis::Script::new(
        r#"
        local val = redis.call("GET", KEYS[1])
        if val then
            redis.call("DEL", KEYS[1])
            return val
        else
            return false
        end
        "#,
    );
    let result: Option<String> = script
        .key(format!("floor:{room_id}"))
        .invoke_async(conn)
        .await
        .map_err(|e| AppError::Internal(format!("redis error: {e}")))?;
    match result {
        Some(s) => {
            let uid: Uuid = s
                .parse()
                .map_err(|e| AppError::Internal(format!("uuid parse: {e}")))?;
            Ok(Some(uid))
        }
        None => Ok(None),
    }
}
