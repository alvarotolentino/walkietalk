use std::hash::{Hash, Hasher};

use tauri::{AppHandle, State};

use crate::audio::engine::AudioEngine;
use crate::state::AppState;

/// Deterministic hash of a string ID to a u64 (for AudioFrame room_id).
fn hash_to_u64(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Initialise the audio engine for a room session.
/// Creates both CPAL input + output streams (paused).
/// Called once when entering a room.
#[tauri::command]
pub async fn init_audio_engine(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut eng = state.audio_engine.lock().await;
    if let Some(old) = eng.take() {
        old.shutdown();
    }
    let receiver = state.audio_rx.clone();
    *eng = Some(AudioEngine::new(app, receiver)?);
    Ok(())
}

/// Shut down the audio engine.
/// Called when leaving a room.
#[tauri::command]
pub async fn shutdown_audio_engine(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut eng = state.audio_engine.lock().await;
    if let Some(engine) = eng.take() {
        engine.shutdown();
    }
    Ok(())
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
    state: State<'_, AppState>,
) -> Result<(), String> {
    let room_id_u64 = lock_key as u64;
    let speaker_id_u32 = hash_to_u64(&user_id) as u32;

    let write_tx = {
        let transport = state.transport.lock().await;
        let t = transport.as_ref().ok_or("Not connected")?;
        t.write_channel().await
    };

    let mut eng = state.audio_engine.lock().await;
    let engine = eng.as_mut().ok_or("Audio engine not initialised")?;
    engine.activate_capture(room_id_u64, speaker_id_u32, write_tx)
}

/// Stop audio capture. Called when floor is released/denied/timed out.
#[tauri::command]
pub async fn stop_audio_capture(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut eng = state.audio_engine.lock().await;
    if let Some(engine) = eng.as_mut() {
        engine.deactivate_capture();
    }
    Ok(())
}

/// Start audio playback. Called when another user is granted the floor.
#[tauri::command]
pub async fn start_audio_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let eng = state.audio_engine.lock().await;
    let engine = eng.as_ref().ok_or("Audio engine not initialised")?;
    engine.activate_playback()
}

/// Stop audio playback. Called when the speaking user releases/times out.
#[tauri::command]
pub async fn stop_audio_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let eng = state.audio_engine.lock().await;
    if let Some(engine) = eng.as_ref() {
        engine.deactivate_playback();
    }
    Ok(())
}
