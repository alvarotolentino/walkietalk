import { type Component, onMount, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { checkAuth } from "../stores/auth";
import { connect } from "../stores/connection";

const Splash: Component = () => {
  const [showSpinner, setShowSpinner] = createSignal(false);

  onMount(async () => {
    const timer = setTimeout(() => setShowSpinner(true), 1000);
    const minDisplay = new Promise((r) => setTimeout(r, 800));

    const [authed] = await Promise.all([checkAuth(), minDisplay]);
    clearTimeout(timer);

    if (authed) {
      connect().catch(() => {});
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
          animation: "splash-scale 500ms var(--ease-spring) both",
          color: "var(--color-brand-primary)",
        }}
        role="img"
        aria-label="WalkieTalk logo"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="11" rx="3"/><path d="M5 10a7 7 0 0 0 14 0"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
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
