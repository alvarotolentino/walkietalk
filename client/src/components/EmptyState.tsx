import { type Component, type JSX, Show } from "solid-js";
import Button from "./Button";

export interface EmptyStateProps {
  title: string;
  description: string;
  action?: { label: string; onClick: () => void };
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
          "font-size": "var(--text-3xl)",
        }}
        aria-hidden="true"
      >
        📻
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
        {(action) => (
          <Button variant="primary" onClick={action().onClick}>
            {action().label}
          </Button>
        )}
      </Show>
    </div>
  );
};

export default EmptyState;
