import { type Component, createUniqueId } from "solid-js";

export interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}

const Toggle: Component<ToggleProps> = (props) => {
  const id = createUniqueId();

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "space-between",
        "min-height": "48px",
      }}
    >
      <label
        for={id}
        style={{
          "font-size": "var(--text-base)",
          color: "var(--color-text-primary)",
          cursor: "pointer",
          flex: "1",
        }}
      >
        {props.label}
      </label>
      <button
        id={id}
        role="switch"
        aria-checked={props.checked}
        onClick={() => props.onChange(!props.checked)}
        style={{
          width: "48px",
          height: "28px",
          "border-radius": "var(--radius-full)",
          border: "none",
          padding: "2px",
          cursor: "pointer",
          background: props.checked ? "var(--color-brand-primary)" : "var(--color-bg-tertiary)",
          transition: "background var(--duration-fast) var(--ease-default)",
          display: "flex",
          "align-items": "center",
          "flex-shrink": "0",
          "margin-left": "var(--space-4)",
        }}
      >
        <div
          style={{
            width: "24px",
            height: "24px",
            "border-radius": "var(--radius-full)",
            background: "#fff",
            transform: props.checked ? "translateX(20px)" : "translateX(0)",
            transition: "transform var(--duration-fast) var(--ease-default)",
            "box-shadow": "var(--shadow-sm)",
          }}
        />
      </button>
    </div>
  );
};

export default Toggle;
