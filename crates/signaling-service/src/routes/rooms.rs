use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::db;
use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;
use walkietalk_shared::ids::RoomId;

use crate::models::room::{
    get_room_member_info, CreateRoomRequest, InviteCodeResponse, JoinRoomRequest, RoomDetailMember,
    RoomDetailResponse, RoomResponse, UpdateRoomRequest,
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

    let slug = generate_slug(&req.name);
    let conn = &mut state.redis.clone();

    let room = db::create_room(
        conn,
        &req.name,
        req.description.as_deref(),
        &slug,
        auth.user_id.0,
    )
    .await?;

    db::add_room_member(conn, room.id, auth.user_id.0, "owner").await?;

    // Auto-generate an invite code so the room is joinable immediately.
    let mut code = generate_invite_code();
    let mut attempts = 0u8;
    loop {
        let was_set = db::set_room_invite_code(conn, room.id, None, &code).await?;
        if was_set {
            break;
        }
        attempts += 1;
        if attempts >= 5 {
            return Err(AppError::Internal(
                "failed to generate a unique invite code".into(),
            ));
        }
        code = generate_invite_code();
    }

    let mut room_with_code = room;
    room_with_code.invite_code = Some(code);

    Ok((
        StatusCode::CREATED,
        Json(RoomResponse::from_record(room_with_code, 1)),
    ))
}

// ---------------------------------------------------------------------------
// GET /rooms
// ---------------------------------------------------------------------------

pub async fn list_rooms(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<RoomResponse>>, AppError> {
    let conn = &mut state.redis.clone();
    let records = db::list_user_rooms(conn, auth.user_id.0).await?;

    let rooms: Vec<RoomResponse> = records
        .into_iter()
        .map(|(rec, count)| RoomResponse::from_record(rec, count))
        .collect();

    Ok(Json(rooms))
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
    let conn = &mut state.redis.clone();

    if !db::is_room_member(conn, room_id, auth.user_id.0).await? {
        return Err(AppError::Forbidden("not a room member".into()));
    }

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    let members_raw = get_room_member_info(conn, &rid).await?;
    let member_count = members_raw.len() as i64;
    let members: Vec<RoomDetailMember> = members_raw
        .into_iter()
        .map(|m| RoomDetailMember {
            user_id: m.user_id.0,
            display_name: m.display_name,
            role: if m.user_id.0 == room.owner_id {
                "owner"
            } else {
                "member"
            },
        })
        .collect();

    Ok(Json(RoomDetailResponse {
        id: room.id,
        slug: room.slug,
        name: room.name,
        description: room.description,
        owner_id: room.owner_id,
        invite_code: room.invite_code,
        created_at: room.created_at,
        member_count,
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

    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden(
            "only the owner can update the room".into(),
        ));
    }

    let name = req.name.as_deref().unwrap_or(&room.name);
    let description = req.description.as_deref().or(room.description.as_deref());

    db::update_room(conn, room_id, name, description).await?;

    let member_count = db::room_member_count(conn, room_id).await?;

    Ok(Json(RoomResponse {
        id: room.id,
        slug: room.slug,
        name: name.to_string(),
        description: description.map(String::from),
        owner_id: room.owner_id,
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
    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden(
            "only the owner can delete the room".into(),
        ));
    }

    db::delete_room(conn, &room).await?;

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
    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    // All rooms require a valid invite code to join
    let provided = req
        .invite_code
        .as_deref()
        .ok_or_else(|| AppError::Forbidden("invite code required".into()))?;
    let expected = room
        .invite_code
        .as_deref()
        .ok_or_else(|| AppError::Forbidden("room has no active invite code".into()))?;
    if provided != expected {
        return Err(AppError::Forbidden("invalid invite code".into()));
    }

    if db::is_room_member(conn, room_id, auth.user_id.0).await? {
        return Err(AppError::Conflict("already a member of this room".into()));
    }

    let count = db::room_member_count(conn, room_id).await?;
    if count >= 500 {
        return Err(AppError::Forbidden("room is full (max 500 members)".into()));
    }

    db::add_room_member(conn, room_id, auth.user_id.0, "member").await?;

    Ok(Json(RoomResponse::from_record(room, count + 1)))
}

// ---------------------------------------------------------------------------
// POST /rooms/join  (join by invite code, no room ID needed)
// ---------------------------------------------------------------------------

pub async fn join_by_code(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<JoinRoomRequest>,
) -> Result<Json<RoomResponse>, AppError> {
    let code = req
        .invite_code
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("invite_code is required".into()))?;

    let conn = &mut state.redis.clone();

    let room_id = db::get_room_id_by_invite_code(conn, code)
        .await?
        .ok_or_else(|| AppError::NotFound("invalid invite code".into()))?;

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if db::is_room_member(conn, room_id, auth.user_id.0).await? {
        return Err(AppError::Conflict("already a member of this room".into()));
    }

    let count = db::room_member_count(conn, room_id).await?;
    if count >= 500 {
        return Err(AppError::Forbidden("room is full (max 500 members)".into()));
    }

    db::add_room_member(conn, room_id, auth.user_id.0, "member").await?;

    Ok(Json(RoomResponse::from_record(room, count + 1)))
}

// ---------------------------------------------------------------------------
// POST /rooms/:id/invite
// ---------------------------------------------------------------------------

pub async fn generate_invite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<Json<InviteCodeResponse>, AppError> {
    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden(
            "only the owner can generate invite codes".into(),
        ));
    }

    // Retry on collision (up to 5 attempts) to guarantee uniqueness.
    let mut code = generate_invite_code();
    let mut attempts = 0u8;
    loop {
        let was_set =
            db::set_room_invite_code(conn, room_id, room.invite_code.as_deref(), &code).await?;
        if was_set {
            break;
        }
        attempts += 1;
        if attempts >= 5 {
            return Err(AppError::Internal(
                "failed to generate a unique invite code".into(),
            ));
        }
        code = generate_invite_code();
    }

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
    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id == auth.user_id.0 {
        return Err(AppError::BadRequest(
            "owner cannot leave; transfer ownership or delete the room".into(),
        ));
    }

    let removed = db::remove_room_member(conn, room_id, auth.user_id.0).await?;
    if !removed {
        return Err(AppError::NotFound("not a member of this room".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
