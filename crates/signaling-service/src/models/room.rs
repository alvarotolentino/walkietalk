use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::ids::{RoomId, UserId};
use walkietalk_shared::messages::MemberInfo;

// ---------------------------------------------------------------------------
// Database row models
// ---------------------------------------------------------------------------

/// Complete row mapping for the `rooms` table.
/// All fields are required by `FromRow` for `SELECT *` queries.
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)] // REASON: fields map complete DB row via FromRow
pub struct Room {
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

/// Projection used when fetching room member info for ROOM_STATE messages.
#[derive(Debug, sqlx::FromRow)]
pub struct RoomMemberRow {
    pub user_id: Uuid,
    pub display_name: String,
}

/// Projection for room list queries that include a member count.
#[derive(Debug, sqlx::FromRow)]
pub struct RoomWithCount {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub visibility: String,
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate)]
pub struct CreateRoomRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    pub description: Option<String>,
    /// "public" or "private" (default "private")
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateRoomRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JoinRoomRequest {
    pub invite_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PublicRoomQuery {
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ---------------------------------------------------------------------------
// Response bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RoomResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub visibility: String,
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
}

impl From<RoomWithCount> for RoomResponse {
    fn from(r: RoomWithCount) -> Self {
        Self {
            id: r.id,
            slug: r.slug,
            name: r.name,
            description: r.description,
            owner_id: r.owner_id,
            visibility: r.visibility,
            invite_code: r.invite_code,
            created_at: r.created_at,
            member_count: r.member_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RoomDetailResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub visibility: String,
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub members: Vec<MemberInfo>,
}

#[derive(Debug, Serialize)]
pub struct InviteCodeResponse {
    pub invite_code: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fetch all room members with their display names (for ROOM_STATE and room detail).
pub async fn get_room_member_info(
    db: &sqlx::PgPool,
    room_id: &RoomId,
) -> Result<Vec<MemberInfo>, walkietalk_shared::error::AppError> {
    let rows = sqlx::query_as::<_, RoomMemberRow>(
        "SELECT rm.user_id, u.display_name \
         FROM room_members rm \
         JOIN users u ON u.id = rm.user_id \
         WHERE rm.room_id = $1",
    )
    .bind(room_id.0)
    .fetch_all(db)
    .await
    .map_err(|e| walkietalk_shared::error::AppError::Internal(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| MemberInfo {
            user_id: UserId(r.user_id),
            display_name: r.display_name,
            status: walkietalk_shared::enums::PresenceStatus::Offline,
        })
        .collect())
}
