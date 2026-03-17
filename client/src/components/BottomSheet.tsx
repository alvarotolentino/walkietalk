import { type Component, type JSX, Show } from "solid-js";

export interface BottomSheetProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: JSX.Element;
}

const BottomSheet: Component<BottomSheetProps> = (props) => {
  const handleOverlayClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) props.onClose();
  };

  return (
    <Show when={props.isOpen}>
      <div
        onClick={handleOverlayClick}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "900",
          display: "flex",
          "flex-direction": "column",
          "justify-content": "flex-end",
          background: "var(--color-bg-overlay)",
          animation: "fadeIn var(--duration-fast) var(--ease-default)",
        }}
      >
        <div
          role="dialog"
          aria-modal="true"
          aria-label={props.title}
          style={{
            background: "var(--color-bg-secondary)",
            "border-radius": "var(--radius-lg) var(--radius-lg) 0 0",
            "max-height": "80vh",
            overflow: "auto",
            animation: "slideUp var(--duration-normal) var(--ease-out)",
          }}
        >
          {/* Handle bar */}
          <div
            style={{
              display: "flex",
              "justify-content": "center",
              padding: "var(--space-3) 0 var(--space-1)",
            }}
          >
            <div
              style={{
                width: "36px",
                height: "4px",
                "border-radius": "var(--radius-full)",
                background: "var(--color-border-default)",
              }}
            />
          </div>

          {/* Header */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "var(--space-2) var(--space-4) var(--space-4)",
            }}
          >
            <h2 style={{ "font-size": "var(--text-xl)", "font-weight": "var(--font-semibold)" }}>
              {props.title}
            </h2>
            <button
              onClick={props.onClose}
              aria-label="Close"
              style={{
                "min-height": "48px",
                "min-width": "48px",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "font-size": "var(--text-xl)",
                color: "var(--color-text-secondary)",
              }}
            >
              ✕
            </button>
          </div>

          {/* Content */}
          <div style={{ padding: "0 var(--space-4) var(--space-6)" }}>
            {props.children}
          </div>
        </div>
      </div>
    </Show>
  );
};

export default BottomSheet;
