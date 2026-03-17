import { type Component } from "solid-js";

export type PresenceState = "online" | "speaking" | "offline";

export interface PresenceDotProps {
  status: PresenceState;
}

const STATUS_COLORS: Record<PresenceState, string> = {
  online: "var(--color-presence-online)",
  speaking: "var(--color-presence-speaking)",
  offline: "var(--color-presence-offline)",
};

const STATUS_LABELS: Record<PresenceState, string> = {
  online: "Online",
  speaking: "Speaking",
  offline: "Offline",
};

const PresenceDot: Component<PresenceDotProps> = (props) => {
  return (
    <span
      role="img"
      aria-label={STATUS_LABELS[props.status]}
      style={{
        display: "inline-block",
        width: "8px",
        height: "8px",
        "border-radius": "var(--radius-full)",
        background: STATUS_COLORS[props.status],
        "flex-shrink": "0",
        animation: props.status === "speaking" ? "pulse 0.67s ease-in-out infinite" : "none",
      }}
    />
  );
};

export default PresenceDot;
