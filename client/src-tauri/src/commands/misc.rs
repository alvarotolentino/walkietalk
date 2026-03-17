/// Miscellaneous commands: haptics, sounds, etc.

#[tauri::command]
pub async fn trigger_haptic(_style: String) -> Result<(), String> {
    // Platform-specific haptic feedback.
    // On desktop (dev mode), this is a no-op.
    // On iOS/Android, this would call into the native haptic APIs.
    #[cfg(target_os = "ios")]
    {
        // iOS haptic implementation would go here via Tauri plugin or Swift bridge
        tracing::debug!("haptic: {_style}");
    }
    #[cfg(target_os = "android")]
    {
        tracing::debug!("haptic: {_style}");
    }
    Ok(())
}

#[tauri::command]
pub async fn play_sound(_name: String) -> Result<(), String> {
    // Sound playback via the audio pipeline.
    // In production, this would load and play WAV files from the app bundle.
    tracing::debug!("play_sound: {_name}");
    Ok(())
}
