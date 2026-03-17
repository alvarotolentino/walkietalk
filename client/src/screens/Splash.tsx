import { type Component, onMount, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { checkAuth } from "../stores/auth";

const Splash: Component = () => {
  const [showSpinner, setShowSpinner] = createSignal(false);

  onMount(async () => {
    const timer = setTimeout(() => setShowSpinner(true), 1000);
    const minDisplay = new Promise((r) => setTimeout(r, 800));

    const [authed] = await Promise.all([checkAuth(), minDisplay]);
    clearTimeout(timer);

    if (authed) {
      navigate(Screen.RoomList);
    } else {
      navigate(Screen.Login);
    }
  });

  return (
    <div
      class="screen"
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        gap: "var(--space-4)",
      }}
    >
      <div
        style={{
          "font-size": "var(--text-3xl)",
          "font-weight": "var(--font-bold)",
          animation: "splash-scale 500ms var(--ease-spring) both",
        }}
        role="img"
        aria-label="WalkieTalk logo"
      >
        📻
      </div>
      <h1
        style={{
          "font-size": "var(--text-2xl)",
          "font-weight": "var(--font-bold)",
        }}
      >
        WalkieTalk
      </h1>
      {showSpinner() && (
        <div
          role="progressbar"
          aria-label="Loading"
          style={{
            width: "24px",
            height: "24px",
            border: "3px solid var(--color-bg-tertiary)",
            "border-top-color": "var(--color-brand-primary)",
            "border-radius": "var(--radius-full)",
            animation: "spin 0.8s linear infinite",
          }}
        />
      )}
      <span class="sr-only">WalkieTalk. Loading.</span>
      <style>{`
        @keyframes splash-scale {
          from { transform: scale(0.8); opacity: 0; }
          to { transform: scale(1); opacity: 1; }
        }
        @keyframes spin {
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
};

export default Splash;
