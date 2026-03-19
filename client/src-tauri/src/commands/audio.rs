use std::hash::{Hash, Hasher};

use tauri::{AppHandle, State};

use crate::audio::{capture, playback};
use crate::state::AppState;

/// Deterministic hash of a string ID to a u64 (for AudioFrame room_id).
fn hash_to_u64(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Start audio capture for the given room. Called when floor is granted.
///
/// `lock_key` is the server-assigned wire room ID used in AudioFrame headers.
/// `user_id` is hashed to a numeric speaker_id for the AudioFrame header.
#[tauri::command]
pub async fn start_audio_capture(
    _room_id: String,
    user_id: String,
    lock_key: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let room_id_u64 = lock_key as u64;
    let speaker_id_u32 = hash_to_u64(&user_id) as u32;
    // Stop any existing capture first.
    let mut cap = state.capture.lock().await;
    if let Some(handle) = cap.take() {
        handle.stop();
    }

    // Get a clone of the WS write channel from the transport.
    let write_tx = {
        let transport = state.transport.lock().await;
        let t = transport.as_ref().ok_or("Not connected")?;
        t.write_channel().await
    };

    let handle = capture::start_capture(app, room_id_u64, speaker_id_u32, write_tx)?;
    *cap = Some(handle);
    Ok(())
}

/// Stop audio capture. Called when floor is released/denied/timed out.
#[tauri::command]
pub async fn stop_audio_capture(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cap = state.capture.lock().await;
    if let Some(handle) = cap.take() {
        handle.stop();
    }
    Ok(())
}

/// Start audio playback. Called when another user is granted the floor.
#[tauri::command]
pub async fn start_audio_playback(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut pb = state.playback.lock().await;
    // Stop any existing playback first
    if let Some(handle) = pb.take() {
        handle.stop();
    }
    let handle = playback::start_playback(app)?;
    *pb = Some(handle);
    Ok(())
}

/// Stop audio playback. Called when the speaking user releases/times out.
#[tauri::command]
pub async fn stop_audio_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut pb = state.playback.lock().await;
    if let Some(handle) = pb.take() {
        handle.stop();
    }
    Ok(())
}
