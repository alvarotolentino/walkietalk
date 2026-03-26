import { type Component, type JSX, Show, createUniqueId } from "solid-js";

export interface InputProps {
  label: string;
  type?: "text" | "email" | "password";
  value: string;
  onInput: (value: string) => void;
  autocomplete?: string;
  placeholder?: string;
  disabled?: boolean;
  error?: string;
  helper?: string;
  style?: JSX.CSSProperties;
  maxLength?: number;
  /** Suffix button inside the input (e.g., show/hide password toggle) */
  suffix?: JSX.Element;
}

const Input: Component<InputProps> = (props) => {
  const id = createUniqueId();
  const errorId = `${id}-error`;
  const helperId = `${id}-helper`;

  const describedBy = () => {
    const parts: string[] = [];
    if (props.error) parts.push(errorId);
    if (props.helper) parts.push(helperId);
    return parts.length > 0 ? parts.join(" ") : undefined;
  };

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-1)" }}>
      <label
        for={id}
        style={{
          "font-size": "var(--text-sm)",
          "font-weight": "var(--font-medium)",
          color: "var(--color-text-secondary)",
        }}
      >
        {props.label}
      </label>
      <div style={{ position: "relative" }}>
        <input
          id={id}
          type={props.type ?? "text"}
          value={props.value}
          onInput={(e) => props.onInput(e.currentTarget.value)}
          autocomplete={props.autocomplete ?? "off"}
          attr:data-form-type="other"
          attr:data-lpignore="true"
          placeholder={props.placeholder}
          disabled={props.disabled}
          maxLength={props.maxLength}
          aria-invalid={!!props.error}
          aria-describedby={describedBy()}
          style={{
            width: "100%",
            "box-sizing": "border-box",
            "min-height": "48px",
            padding: "var(--space-3) var(--space-4)",
            "padding-right": props.suffix ? "var(--space-12)" : "var(--space-4)",
            background: "var(--color-bg-tertiary)",
            color: "var(--color-text-primary)",
            border: `1px solid ${props.error ? "var(--color-error)" : "var(--color-border-default)"}`,
            "border-radius": "var(--radius-md)",
            "font-size": "var(--text-base)",
            "font-family": "var(--font-family)",
            opacity: 1,
            "-webkit-text-fill-color": "var(--color-text-primary)",
            outline: "none",
            transition: "border-color var(--duration-fast) var(--ease-default)",
            ...props.style,
          }}
        />
        <Show when={props.suffix}>
          <div
            style={{
              position: "absolute",
              right: "var(--space-2)",
              top: "50%",
              transform: "translateY(-50%)",
            }}
          >
            {props.suffix}
          </div>
        </Show>
      </div>
      <Show when={props.error}>
        <div
          id={errorId}
          role="alert"
          style={{ "font-size": "var(--text-sm)", color: "var(--color-error)" }}
        >
          {props.error}
        </div>
      </Show>
      <Show when={props.helper && !props.error}>
        <div
          id={helperId}
          style={{ "font-size": "var(--text-sm)", color: "var(--color-text-tertiary)" }}
        >
          {props.helper}
        </div>
      </Show>
    </div>
  );
};

export default Input;
