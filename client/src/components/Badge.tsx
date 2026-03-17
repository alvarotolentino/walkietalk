import { type Component } from "solid-js";

export interface BadgeProps {
  text: string;
  variant?: "default" | "primary" | "success" | "warning";
}

const VARIANT_STYLES: Record<string, { bg: string; color: string }> = {
  default: { bg: "var(--color-bg-tertiary)", color: "var(--color-text-secondary)" },
  primary: { bg: "var(--color-brand-primary)", color: "#fff" },
  success: { bg: "var(--color-success)", color: "#fff" },
  warning: { bg: "var(--color-warning)", color: "var(--color-text-inverse)" },
};

const Badge: Component<BadgeProps> = (props) => {
  const v = () => VARIANT_STYLES[props.variant ?? "default"];

  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        "padding-left": "var(--space-2)",
        "padding-right": "var(--space-2)",
        height: "20px",
        "font-size": "var(--text-xs)",
        "font-weight": "var(--font-medium)",
        "border-radius": "var(--radius-sm)",
        background: v().bg,
        color: v().color,
        "white-space": "nowrap",
      }}
    >
      {props.text}
    </span>
  );
};

export default Badge;
