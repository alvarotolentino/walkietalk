use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use walkietalk_shared::db;
use walkietalk_shared::error::AppError;
use walkietalk_shared::extractors::AuthUser;
use walkietalk_shared::ids::RoomId;

use crate::models::room::{
    CreateRoomRequest, InviteCodeResponse, JoinRoomRequest, PublicRoomQuery,
    RoomDetailResponse, RoomResponse, UpdateRoomRequest, get_room_member_info,
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
    let conn = &mut state.redis.clone();

    let room = db::create_room(
        conn,
        &req.name,
        req.description.as_deref(),
        &slug,
        auth.user_id.0,
        visibility,
    )
    .await?;

    db::add_room_member(conn, room.id, auth.user_id.0, "owner").await?;

    Ok((
        StatusCode::CREATED,
        Json(RoomResponse::from_record(room, 1)),
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
// GET /rooms/public
// ---------------------------------------------------------------------------

pub async fn list_public_rooms(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(params): Query<PublicRoomQuery>,
) -> Result<Json<Vec<RoomResponse>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0).max(0);
    let conn = &mut state.redis.clone();

    let records = db::list_public_rooms(conn, params.search.as_deref(), limit, offset).await?;

    let mut rooms = Vec::with_capacity(records.len());
    for (rec, count) in records {
        rooms.push(RoomResponse::from_record(rec, count));
    }

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

    let members = get_room_member_info(conn, &rid).await?;

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

    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
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

    db::update_room(conn, room_id, name, description, visibility, &room.visibility).await?;

    let member_count = db::room_member_count(conn, room_id).await?;

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
    let conn = &mut state.redis.clone();

    let room = db::get_room(conn, room_id)
        .await?
        .ok_or_else(|| AppError::NotFound("room not found".into()))?;

    if room.owner_id != auth.user_id.0 {
        return Err(AppError::Forbidden("only the owner can delete the room".into()));
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
        return Err(AppError::Forbidden("only the owner can generate invite codes".into()));
    }

    let code = generate_invite_code();
    db::set_room_invite_code(conn, room_id, room.invite_code.as_deref(), &code).await?;

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
