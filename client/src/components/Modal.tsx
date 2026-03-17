import { type Component, type JSX, Show } from "solid-js";
import Button from "./Button";

export interface ModalProps {
  title: string;
  message?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "danger" | "primary";
  onConfirm: () => void;
  onCancel: () => void;
}

const Modal: Component<ModalProps> = (props) => {
  const handleOverlayClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) props.onCancel();
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="modal-title"
      onClick={handleOverlayClick}
      style={{
        position: "fixed",
        inset: "0",
        "z-index": "1000",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        background: "var(--color-bg-overlay)",
        padding: "var(--space-6)",
        animation: "fadeIn var(--duration-fast) var(--ease-default)",
      }}
    >
      <div
        style={{
          background: "var(--color-bg-secondary)",
          "border-radius": "var(--radius-lg)",
          padding: "var(--space-6)",
          "max-width": "340px",
          width: "100%",
          "box-shadow": "var(--shadow-xl)",
          animation: "scaleIn var(--duration-normal) var(--ease-spring)",
        }}
      >
        <h2
          id="modal-title"
          style={{
            "font-size": "var(--text-lg)",
            "font-weight": "var(--font-semibold)",
            "margin-bottom": "var(--space-2)",
          }}
        >
          {props.title}
        </h2>
        <Show when={props.message}>
          <p
            style={{
              "font-size": "var(--text-sm)",
              color: "var(--color-text-secondary)",
              "margin-bottom": "var(--space-5)",
            }}
          >
            {props.message}
          </p>
        </Show>
        <div style={{ display: "flex", gap: "var(--space-3)", "justify-content": "flex-end" }}>
          <Button variant="ghost" onClick={props.onCancel}>
            {props.cancelLabel ?? "Cancel"}
          </Button>
          <Button
            variant={props.variant ?? "danger"}
            onClick={props.onConfirm}
          >
            {props.confirmLabel ?? "Confirm"}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default Modal;
