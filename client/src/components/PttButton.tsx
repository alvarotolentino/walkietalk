import { type Component, createSignal, createMemo, createEffect, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { isTransmitting, sendLevel, floorTimeRemaining } from "../stores/audio";
import { connectionState } from "../stores/connection";
import { floorHolder } from "../stores/activeRoom";
import { user } from "../stores/auth";
import VuMeter from "./VuMeter";
import Countdown from "./Countdown";
import { triggerHaptic } from "../utils/haptics";
import { useTauriEvent } from "../hooks/useTauriEvent";

export type PttState = "idle" | "requesting" | "transmitting" | "occupied" | "disconnected";

export interface PttButtonProps {
  roomId: string;
  speakerName?: string;
  isConnected: boolean;
}

const REQUEST_TIMEOUT_MS = 5000;

const PttButton: Component<PttButtonProps> = (props) => {
  const [requesting, setRequesting] = createSignal(false);
  let requestTimer: ReturnType<typeof setTimeout> | null = null;

  // Clear requesting state when the floor is granted or denied
  // (floor_granted → isTransmitting becomes true; floor_denied → floorHolder stays null)
  createEffect(() => {
    if (isTransmitting()) {
      setRequesting(false);
      clearRequestTimer();
    }
  });

  function clearRequestTimer() {
    if (requestTimer) {
      clearTimeout(requestTimer);
      requestTimer = null;
    }
  }

  onCleanup(() => clearRequestTimer());

  // Clear requesting when floor is denied by server
  useTauriEvent("floor_denied", () => {
    setRequesting(false);
    clearRequestTimer();
  });

  // Also clear requesting on server errors (e.g. "not in this room")
  useTauriEvent("server_error", () => {
    setRequesting(false);
    clearRequestTimer();
  });

  const pttState = createMemo<PttState>(() => {
    if (!props.isConnected || connectionState() !== "connected") return "disconnected";
    if (isTransmitting()) return "transmitting";
    if (requesting()) return "requesting";
    const holder = floorHolder();
    const me = user();
    if (holder && me && holder !== me.id) return "occupied";
    return "idle";
  });

  const stateConfig: Record<PttState, { bg: string; label: string; icon: string }> = {
    idle: { bg: "var(--color-ptt-idle)", label: "Hold to talk", icon: "🎤" },
    requesting: { bg: "var(--color-ptt-requesting)", label: "Requesting...", icon: "⏳" },
    transmitting: { bg: "var(--color-ptt-transmitting)", label: "Release to stop", icon: "🎤" },
    occupied: { bg: "var(--color-ptt-occupied)", label: `${props.speakerName ?? "Someone"} is talking`, icon: "🔒" },
    disconnected: { bg: "var(--color-ptt-disabled)", label: "Not connected", icon: "📵" },
  };

  const cfg = () => stateConfig[pttState()];
  const isDisabled = () => pttState() === "occupied" || pttState() === "disconnected";

  const handlePressStart = async () => {
    if (isDisabled()) {
      if (pttState() === "occupied") {
        triggerHaptic("rigid");
      }
      return;
    }
    if (pttState() === "transmitting") return;

    triggerHaptic("light");
    setRequesting(true);

    // Safety timeout: if server never responds with granted/denied, reset
    clearRequestTimer();
    requestTimer = setTimeout(() => {
      if (requesting()) {
        setRequesting(false);
        triggerHaptic("error");
      }
    }, REQUEST_TIMEOUT_MS);

    try {
      await invoke("request_floor", { roomId: props.roomId });
    } catch {
      setRequesting(false);
      clearRequestTimer();
    }
  };

  const handlePressEnd = async () => {
    if (pttState() === "transmitting") {
      triggerHaptic("light");
      try {
        await invoke("release_floor", { roomId: props.roomId });
      } catch {
        // Floor release failed; server timeout will clean up
      }
    }
    setRequesting(false);
    clearRequestTimer();
  };

  // The requesting flag is cleared when we transition to transmitting or idle
  // (the store update from floor_granted/denied event handles the actual state)

  // Keyboard support: Space/Enter to toggle PTT (spec §9.10)
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      if (pttState() === "idle") {
        handlePressStart();
      }
    }
  };

  const handleKeyUp = (e: KeyboardEvent) => {
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      handlePressEnd();
    }
  };

  const ariaLabel = createMemo(() => {
    switch (pttState()) {
      case "idle": return "Push to talk. Hold to speak.";
      case "requesting": return "Requesting floor. Please wait.";
      case "transmitting": return "Transmitting. Release to stop.";
      case "occupied": return `${props.speakerName ?? "Someone"} is speaking. Push to talk disabled.`;
      case "disconnected": return "Not connected. Push to talk disabled.";
    }
  });

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        gap: "var(--space-3)",
      }}
    >
      <Show when={pttState() === "transmitting"}>
        <VuMeter level={sendLevel()} variant="send" />
      </Show>
      <button
        role="button"
        aria-label={ariaLabel()}
        aria-pressed={pttState() === "transmitting"}
        aria-disabled={isDisabled()}
        onPointerDown={handlePressStart}
        onPointerUp={handlePressEnd}
        onPointerLeave={handlePressEnd}
        onKeyDown={handleKeyDown}
        onKeyUp={handleKeyUp}
        onContextMenu={(e) => e.preventDefault()}
        style={{
          width: "140px",
          height: "140px",
          "border-radius": "var(--radius-full)",
          background: cfg().bg,
          border: "none",
          cursor: isDisabled() ? "not-allowed" : "pointer",
          display: "flex",
          "flex-direction": "column",
          "align-items": "center",
          "justify-content": "center",
          gap: "var(--space-1)",
          "font-size": "var(--text-3xl)",
          color: "#fff",
          "box-shadow": pttState() === "transmitting"
            ? `0 0 0 8px ${cfg().bg}44, 0 0 30px ${cfg().bg}66`
            : "var(--shadow-lg)",
          animation: pttState() === "transmitting" ? "pttPulse 1s ease-in-out infinite" : "none",
          transition: "background var(--duration-fast) var(--ease-default), box-shadow var(--duration-fast) var(--ease-default)",
          "touch-action": "none",
          "user-select": "none",
          "-webkit-user-select": "none",
        }}
      >
        <span style={{ "font-size": "40px" }} aria-hidden="true">
          {cfg().icon}
        </span>
      </button>
      <span
        style={{
          "font-size": "var(--text-sm)",
          color: "var(--color-text-secondary)",
          "text-align": "center",
        }}
      >
        {cfg().label}
      </span>
      <Show when={pttState() === "transmitting"}>
        <Countdown seconds={floorTimeRemaining()} />
      </Show>
    </div>
  );
};

export default PttButton;
