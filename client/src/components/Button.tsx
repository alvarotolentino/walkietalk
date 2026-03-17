import { type Component, type JSX, Show } from "solid-js";

export interface ButtonProps {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  disabled?: boolean;
  loading?: boolean;
  type?: "button" | "submit";
  fullWidth?: boolean;
  onClick?: () => void;
  children: JSX.Element;
}

const Button: Component<ButtonProps> = (props) => {
  const variant = () => props.variant ?? "primary";

  const baseStyle: JSX.CSSProperties = {
    display: "inline-flex",
    "align-items": "center",
    "justify-content": "center",
    gap: "var(--space-2)",
    "min-height": "48px",
    "padding-left": "var(--space-5)",
    "padding-right": "var(--space-5)",
    "font-size": "var(--text-base)",
    "font-weight": "var(--font-semibold)",
    "font-family": "var(--font-family)",
    "border-radius": "var(--radius-md)",
    border: "none",
    cursor: "pointer",
    transition: `background var(--duration-fast) var(--ease-default),
                 opacity var(--duration-fast) var(--ease-default)`,
    "user-select": "none",
    "-webkit-user-select": "none",
    width: props.fullWidth ? "100%" : "auto",
    opacity: (props.disabled || props.loading) ? "0.5" : "1",
    "pointer-events": (props.disabled || props.loading) ? "none" : "auto",
  };

  const variantStyles: Record<string, JSX.CSSProperties> = {
    primary: {
      background: "var(--color-brand-primary)",
      color: "#fff",
    },
    secondary: {
      background: "var(--color-bg-tertiary)",
      color: "var(--color-text-primary)",
    },
    ghost: {
      background: "transparent",
      color: "var(--color-brand-primary)",
    },
    danger: {
      background: "var(--color-error)",
      color: "#fff",
    },
  };

  return (
    <button
      type={props.type ?? "button"}
      disabled={props.disabled || props.loading}
      onClick={props.onClick}
      style={{ ...baseStyle, ...variantStyles[variant()] }}
      aria-busy={props.loading}
    >
      <Show when={props.loading}>
        <span
          style={{
            width: "16px",
            height: "16px",
            border: "2px solid currentColor",
            "border-top-color": "transparent",
            "border-radius": "var(--radius-full)",
            animation: "spin 0.6s linear infinite",
          }}
          aria-hidden="true"
        />
      </Show>
      {props.children}
    </button>
  );
};

export default Button;
