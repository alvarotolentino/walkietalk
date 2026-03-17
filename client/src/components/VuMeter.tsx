import { type Component, createMemo } from "solid-js";

export interface VuMeterProps {
  /** Audio level normalized 0.0–1.0 */
  level: number;
  variant: "send" | "receive";
}

const VuMeter: Component<VuMeterProps> = (props) => {
  const pct = createMemo(() => Math.round(props.level * 100));
  const color = () =>
    props.variant === "send" ? "var(--color-ptt-transmitting)" : "var(--color-brand-primary)";

  return (
    <div
      role="meter"
      aria-valuenow={pct()}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`Audio level: ${pct()} percent`}
      style={{
        width: "100%",
        "max-width": "200px",
        height: "6px",
        background: "var(--color-bg-tertiary)",
        "border-radius": "var(--radius-full)",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          width: `${pct()}%`,
          height: "100%",
          background: color(),
          "border-radius": "var(--radius-full)",
          transition: "width 50ms linear",
        }}
      />
    </div>
  );
};

export default VuMeter;
