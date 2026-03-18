import { type Component, createMemo, createSignal, createEffect, Show, onCleanup } from "solid-js";
import { connectionState, reconnectAttempt, reconnect } from "../stores/connection";

export type ConnectionStatus = "connected" | "connecting" | "reconnecting" | "disconnected" | "failed";

const STATUS_CONFIG: Record<ConnectionStatus, { color: string }> = {
  connected: { color: "var(--color-connection-connected)" },
  connecting: { color: "var(--color-connection-connecting)" },
  reconnecting: { color: "var(--color-connection-reconnecting)" },
  disconnected: { color: "var(--color-connection-disconnected)" },
  failed: { color: "var(--color-connection-disconnected)" },
};

const ConnectionBar: Component = () => {
  const state = connectionState;
  const attempt = reconnectAttempt;

  const label = createMemo(() => {
    switch (state()) {
      case "connected": return "Connected";
      case "connecting": return "Connecting…";
      case "reconnecting": return `Reconnecting… (attempt ${attempt()})`;
      case "failed": return "Connection failed";
      case "disconnected": return "Disconnected";
      default: return "";
    }
  });

  const cfg = () => STATUS_CONFIG[state() as ConnectionStatus] ?? STATUS_CONFIG.disconnected;

  // Auto-hide when connected after 3 seconds
  const [visible, setVisible] = createSignal(true);

  // Re-show on state change
  createEffect(() => {
    const s = state();
    setVisible(true);
    if (s === "connected") {
      const timer = setTimeout(() => setVisible(false), 3000);
      onCleanup(() => clearTimeout(timer));
    }
  });

  return (
    <Show when={visible()}>
      <div
        role="status"
        aria-live="polite"
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          gap: "var(--space-2)",
          padding: "var(--space-2) var(--space-4)",
          background: cfg().color,
          "font-size": "var(--text-xs)",
          "font-weight": "var(--font-medium)",
          color: "#fff",
          transition: "background var(--duration-fast) var(--ease-default)",
        }}
      >
        <span
          style={{
            width: "6px",
            height: "6px",
            "border-radius": "var(--radius-full)",
            background: "#fff",
          }}
          aria-hidden="true"
        />
        {label()}
        <Show when={state() === "disconnected" || state() === "failed"}>
          <button
            onClick={() => reconnect()}
            style={{
              "margin-left": "var(--space-2)",
              padding: "var(--space-1) var(--space-3)",
              background: "rgba(255,255,255,0.2)",
              color: "#fff",
              "border-radius": "var(--radius-sm)",
              border: "none",
              "font-size": "var(--text-xs)",
              "font-weight": "var(--font-semibold)",
              cursor: "pointer",
              "min-height": "28px",
            }}
          >
            Retry
          </button>
        </Show>
      </div>
    </Show>
  );
};

export default ConnectionBar;
