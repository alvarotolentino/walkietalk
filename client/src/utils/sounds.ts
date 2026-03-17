import { invoke } from "@tauri-apps/api/core";

/**
 * Play a named UI sound (e.g., "busy", "connect", "chirp").
 * Sound files live in public/sounds/ and are played by the Rust backend.
 */
export function playSound(name: string) {
  invoke("play_sound", { name }).catch(() => {
    // Non-critical — silently ignore
  });
}
