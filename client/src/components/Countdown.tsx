import { type Component, createMemo } from "solid-js";

export interface CountdownProps {
  /** Remaining seconds */
  seconds: number;
}

const Countdown: Component<CountdownProps> = (props) => {
  const display = createMemo(() => {
    const s = Math.max(0, Math.floor(props.seconds));
    const m = Math.floor(s / 60);
    const sec = s % 60;
    return `${m}:${sec.toString().padStart(2, "0")}`;
  });

  const isWarning = () => props.seconds <= 10 && props.seconds > 0;

  return (
    <span
      role="timer"
      aria-label={`${Math.max(0, Math.floor(props.seconds))} seconds remaining`}
      style={{
        "font-size": "var(--text-sm)",
        "font-weight": "var(--font-medium)",
        "font-variant-numeric": "tabular-nums",
        color: isWarning() ? "var(--color-warning)" : "var(--color-text-secondary)",
        "white-space": "nowrap",
        "flex-shrink": "0",
      }}
    >
      {display()}
    </span>
  );
};

export default Countdown;
