import { type Component, type JSX, Show } from "solid-js";
import Button from "./Button";

export interface EmptyStateProps {
  title: string;
  description: string;
  action?: { label: string; onClick: () => void } | JSX.Element;
}

const EmptyState: Component<EmptyStateProps> = (props) => {
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        padding: "var(--space-10) var(--space-6)",
        "text-align": "center",
        gap: "var(--space-4)",
      }}
    >
      {/* Placeholder illustration */}
      <div
        style={{
          width: "80px",
          height: "80px",
          "border-radius": "var(--radius-full)",
          background: "var(--color-bg-tertiary)",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          color: "var(--color-text-secondary)",
        }}
        aria-hidden="true"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="11" rx="3"/><path d="M5 10a7 7 0 0 0 14 0"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
      </div>
      <div>
        <h3
          style={{
            "font-size": "var(--text-lg)",
            "font-weight": "var(--font-semibold)",
            "margin-bottom": "var(--space-2)",
          }}
        >
          {props.title}
        </h3>
        <p style={{ "font-size": "var(--text-sm)", color: "var(--color-text-secondary)" }}>
          {props.description}
        </p>
      </div>
      <Show when={props.action}>
        {(action) => {
          const a = action();
          if (typeof a === "object" && a !== null && "label" in a && "onClick" in a) {
            const btn = a as { label: string; onClick: () => void };
            return <Button variant="primary" onClick={btn.onClick}>{btn.label}</Button>;
          }
          return a as JSX.Element;
        }}
      </Show>
    </div>
  );
};

export default EmptyState;
