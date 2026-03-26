import { type Component, Show, createMemo } from "solid-js";
import VuMeter from "./VuMeter";
import Countdown from "./Countdown";
import Avatar from "./Avatar";

export interface FloorBannerProps {
  speakerName: string;
  isSelf: boolean;
  level: number;
  timeRemaining: number;
}

const FloorBanner: Component<FloorBannerProps> = (props) => {
  const label = createMemo(() =>
    props.isSelf ? "You are speaking" : `${props.speakerName} is speaking`
  );

  return (
    <div
      role="status"
      aria-live="assertive"
      aria-label={label()}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "var(--space-3)",
        padding: "var(--space-3) var(--space-4)",
        background: "var(--color-bg-secondary)",
        "border-radius": "var(--radius-md)",
        "border-left": "3px solid var(--color-presence-speaking)",
        animation: "slideInDown var(--duration-normal) var(--ease-out)",
      }}
    >
      <Avatar name={props.speakerName} size="sm" />
      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            "font-size": "var(--text-sm)",
            "font-weight": "var(--font-semibold)",
            "white-space": "nowrap",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            display: "flex",
            "align-items": "center",
            gap: "var(--space-1)",
          }}
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="11" rx="3"/><path d="M5 10a7 7 0 0 0 14 0"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
          {label()}
        </div>
        <div style={{ "margin-top": "var(--space-1)" }}>
          <VuMeter level={props.level} variant="receive" />
        </div>
      </div>
      <Countdown seconds={props.timeRemaining} />
    </div>
  );
};

export default FloorBanner;
