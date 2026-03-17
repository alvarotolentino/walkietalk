import { type Component, createSignal, For, Show, onCleanup } from "solid-js";

export type ToastVariant = "info" | "error" | "success";

interface ToastItem {
  id: number;
  message: string;
  variant: ToastVariant;
}

const [toasts, setToasts] = createSignal<ToastItem[]>([]);
let nextId = 0;

export function showToast(message: string, variant: ToastVariant = "info") {
  const id = nextId++;
  setToasts((prev) => [...prev, { id, message, variant }]);
  setTimeout(() => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, 3000);
}

const VARIANT_COLORS: Record<ToastVariant, string> = {
  info: "var(--color-info)",
  error: "var(--color-error)",
  success: "var(--color-success)",
};

const Toast: Component = () => {
  return (
    <div
      aria-live="polite"
      aria-atomic="true"
      style={{
        position: "fixed",
        top: "calc(env(safe-area-inset-top, 0px) + var(--space-4))",
        left: "var(--space-4)",
        right: "var(--space-4)",
        "z-index": "9999",
        display: "flex",
        "flex-direction": "column",
        gap: "var(--space-2)",
        "pointer-events": "none",
      }}
    >
      <For each={toasts()}>
        {(toast) => (
          <div
            role="alert"
            style={{
              padding: "var(--space-3) var(--space-4)",
              background: "var(--color-bg-secondary)",
              "border-left": `3px solid ${VARIANT_COLORS[toast.variant]}`,
              "border-radius": "var(--radius-md)",
              "box-shadow": "var(--shadow-md)",
              color: "var(--color-text-primary)",
              "font-size": "var(--text-sm)",
              animation: "slideInDown var(--duration-normal) var(--ease-out)",
              "pointer-events": "auto",
            }}
          >
            {toast.message}
          </div>
        )}
      </For>
    </div>
  );
};

export default Toast;
