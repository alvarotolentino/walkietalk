import { invoke } from "@tauri-apps/api/core";

export type HapticStyle = "light" | "medium" | "heavy" | "rigid" | "soft" | "error";

/**
 * Trigger platform haptic feedback.
 * Calls into the Tauri Rust backend; no-ops silently on failure.
 */
export function triggerHaptic(style: HapticStyle) {
  invoke("trigger_haptic", { style }).catch(() => {
    // Non-critical — silently ignore if haptics unavailable
  });
}
