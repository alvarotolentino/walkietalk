use std::hash::{Hash, Hasher};

use tauri::{AppHandle, State};

use crate::audio::capture;
use crate::state::AppState;

/// Deterministic hash of a string ID to a u64 (for AudioFrame room_id).
fn hash_to_u64(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Start audio capture for the given room. Called when floor is granted.
///
/// Accepts string IDs (matching the WebSocket protocol) and hashes them
/// to the numeric values used in AudioFrame headers.
#[tauri::command]
pub async fn start_audio_capture(
    room_id: String,
    user_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let room_id_u64 = hash_to_u64(&room_id);
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
        t.write_channel()
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
