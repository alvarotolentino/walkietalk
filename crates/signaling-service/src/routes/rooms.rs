use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;
use walkietalk_shared::ids::RoomId;

use crate::models::room::{
    CreateRoomRequest, InviteCodeResponse, JoinRoomRequest, PublicRoomQuery, Room,
    RoomDetailResponse, RoomResponse, RoomWithCount, UpdateRoomRequest, get_room_member_info,
};
use crate::state::AppState;
use crate::utils::{generate_invite_code, generate_slug};

// ---------------------------------------------------------------------------
// POST /rooms
// ---------------------------------------------------------------------------

pub async fn create_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<RoomResponse>), AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let visibility = req.visibility.as_deref().unwrap_or("private");
    if visibility != "public" && visibility != "private" {
        return Err(AppError::BadRequest(
            "visibility must be 'public' or 'private'".into(),
        ));
    }

    let slug = generate_slug(&req.name);

    let room = sqlx::query_as::<_, Room>(
        "INSERT INTO rooms (name, description, slug, owner_id, visibility) \
         VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&slug)
    .bind(auth.user_id.0)
    .bind(visibility)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    // Auto-insert owner as a member with role 'owner'
    sqlx::query(
        "INSERT INTO room_members (room_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(room.id)
    .bind(auth.user_id.0)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(RoomResponse {
            id: room.id,
            slug: room.slug,
            name: room.name,
            description: room.description,
            owner_id: room.owner_id,
            visibility: room.visibility,
            invite_code: room.invite_code,
            created_at: room.created_at,
            member_count: 1,
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /rooms
// ---------------------------------------------------------------------------

pub async fn list_rooms(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<RoomResponse>>, AppError> {
    let rooms = sqlx::query_as::<_, RoomWithCount>(
        "SELECT r.id, r.slug, r.name, r.description, r.owner_id, r.visibility, \
                r.invite_code, r.created_at, \
                COUNT(rm2.user_id) AS member_count \
         FROM rooms r \
         JOIN room_members rm ON rm.room_id = r.id AND rm.user_id = $1 \
         LEFT JOIN room_members rm2 ON rm2.room_id = r.id \
         GROUP BY r.id \
         ORDER BY r.created_at DESC",
    )
    .bind(auth.user_id.0)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(rooms.into_iter().map(RoomResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// GET /rooms/public
// ---------------------------------------------------------------------------

pub async fn list_public_rooms(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(params): Query<PublicRoomQuery>,
) -> Result<Json<Vec<RoomResponse>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0).max(0);

    let rooms = if let Some(ref search) = params.search {
        let pattern = format!("%{search}%");
        sqlx::query_as::<_, RoomWithCount>(
            "SELECT r.id, r.slug, r.name, r.description, r.owner_id, r.visibility, \
                    r.invite_code, r.created_at, \
                    COUNT(rm.user_id) AS member_count \
             FROM rooms r \
             LEFT JOIN room_members rm ON rm.room_id = r.id \
             WHERE r.visibility = 'public' AND r.name ILIKE $1 \
             GROUP BY r.id \
             ORDER BY r.created_at DESC \
             LIMIT $2 OFFSET $3",
        )
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        sqlx::query_as::<_, RoomWithCount>(
            "SELECT r.id, r.slug, r.name, r.description, r.owner_id, r.visibility, \
                    r.invite_code, r.created_at, \
                    COUNT(rm.user_id) AS member_count \
             FROM rooms r \
             LEFT JOIN room_members rm ON rm.room_id = r.id \
             WHERE r.visibility = 'public' \
             GROUP BY r.id \
             ORDER BY r.created_at DESC \
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    };

    Ok(Json(rooms.into_iter().map(RoomResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// GET /rooms/:id
// ---------------------------------------------------------------------------

pub async fn get_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<Json<RoomDetailResponse>, AppError> {
    let rid = RoomId(room_id);

    // Verify membership
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2)",
    )
    .bind(room_id)
    .bind(auth.user_id.0)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if !is_member {
        return Err(AppError::Forbidden("not a room member".into()));
    }

    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    let members = get_room_member_info(&state.db, &rid).await?;

    Ok(Json(RoomDetailResponse {
        id: room.id,
        slug: room.slug,
        name: room.name,
        description: room.description,
        owner_id: room.owner_id,
        visibility: room.visibility,
        invite_code: room.invite_code,
        created_at: room.created_at,
        members,
    }))
}

// ---------------------------------------------------------------------------
// PATCH /rooms/:id
// ---------------------------------------------------------------------------

pub async fn update_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<UpdateRoomRequest>,
) -> Result<Json<RoomResponse>, AppError> {
    req.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // Verify owner
    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden("only the owner can update the room".into()));
    }

    if let Some(ref v) = req.visibility {
        if v != "public" && v != "private" {
            return Err(AppError::BadRequest(
                "visibility must be 'public' or 'private'".into(),
            ));
        }
    }

    let name = req.name.as_deref().unwrap_or(&room.name);
    let description = req.description.as_deref().or(room.description.as_deref());
    let visibility = req.visibility.as_deref().unwrap_or(&room.visibility);

    sqlx::query(
        "UPDATE rooms SET name = $1, description = $2, visibility = $3, updated_at = NOW() \
         WHERE id = $4",
    )
    .bind(name)
    .bind(description)
    .bind(visibility)
    .bind(room_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        id: room.id,
        slug: room.slug,
        name: name.to_string(),
        description: description.map(String::from),
        owner_id: room.owner_id,
        visibility: visibility.to_string(),
        invite_code: room.invite_code,
        created_at: room.created_at,
        member_count,
    }))
}

// ---------------------------------------------------------------------------
// DELETE /rooms/:id
// ---------------------------------------------------------------------------

pub async fn delete_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden("only the owner can delete the room".into()));
    }

    sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(room_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /rooms/:id/join
// ---------------------------------------------------------------------------

pub async fn join_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<JoinRoomRequest>,
) -> Result<Json<RoomResponse>, AppError> {
    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    // Check access
    if room.visibility == "private" {
        let provided = req
            .invite_code
            .as_deref()
            .ok_or_else(|| AppError::Forbidden("invite code required for private rooms".into()))?;
        let expected = room
            .invite_code
            .as_deref()
            .ok_or_else(|| AppError::Forbidden("room has no active invite code".into()))?;
        if provided != expected {
            return Err(AppError::Forbidden("invalid invite code".into()));
        }
    }

    // Check not already a member
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_members WHERE room_id = $1 AND user_id = $2)",
    )
    .bind(room_id)
    .bind(auth.user_id.0)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if already {
        return Err(AppError::Conflict("already a member of this room".into()));
    }

    // Check member count limit
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    if count >= 500 {
        return Err(AppError::Forbidden("room is full (max 500 members)".into()));
    }

    sqlx::query("INSERT INTO room_members (room_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(room_id)
        .bind(auth.user_id.0)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        id: room.id,
        slug: room.slug,
        name: room.name,
        description: room.description,
        owner_id: room.owner_id,
        visibility: room.visibility,
        invite_code: room.invite_code,
        created_at: room.created_at,
        member_count: count + 1,
    }))
}

// ---------------------------------------------------------------------------
// POST /rooms/:id/invite
// ---------------------------------------------------------------------------

pub async fn generate_invite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<Json<InviteCodeResponse>, AppError> {
    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden("only the owner can generate invite codes".into()));
    }

    let code = generate_invite_code();

    sqlx::query("UPDATE rooms SET invite_code = $1, updated_at = NOW() WHERE id = $2")
        .bind(&code)
        .bind(room_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(InviteCodeResponse { invite_code: code }))
}

// ---------------------------------------------------------------------------
// DELETE /rooms/:id/leave
// ---------------------------------------------------------------------------

pub async fn leave_room(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id == auth.user_id.0 {
        return Err(AppError::BadRequest(
            "owner cannot leave; transfer ownership or delete the room".into(),
        ));
    }

    let result =
        sqlx::query("DELETE FROM room_members WHERE room_id = $1 AND user_id = $2")
            .bind(room_id)
            .bind(auth.user_id.0)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("not a member of this room".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
