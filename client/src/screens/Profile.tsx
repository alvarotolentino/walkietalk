import { type Component, createSignal } from "solid-js";
import { goBack } from "../router";
import { user, logout } from "../stores/auth";
import { getServerUrl, setServerUrl, getSignalingUrl, setSignalingUrl } from "../stores/settings";
import Avatar from "../components/Avatar";
import Button from "../components/Button";
import Input from "../components/Input";
import Toggle from "../components/Toggle";
import Toast, { showToast } from "../components/Toast";
import { navigate, Screen } from "../router";

const Profile: Component = () => {
  const [editingName, setEditingName] = createSignal(false);
  const [displayName, setDisplayName] = createSignal(user()?.display_name ?? "");
  const [chirpEnabled, setChirpEnabled] = createSignal(true);
  const [hapticsEnabled, setHapticsEnabled] = createSignal(true);
  const [serverUrl, setServerUrlLocal] = createSignal(getServerUrl());
  const [signalingUrlVal, setSignalingUrlLocal] = createSignal(getSignalingUrl());

  const handleLogout = async () => {
    await logout();
    navigate(Screen.Login);
  };

  const handleSaveUrl = () => {
    setServerUrl(serverUrl());
    setSignalingUrl(signalingUrlVal());
    showToast("Server URLs updated.", "info");
  };

  return (
    <div class="screen scrollable" style={{ "flex-direction": "column" }}>
      <header
        style={{
          display: "flex",
          "align-items": "center",
          gap: "var(--space-3)",
          padding: "var(--space-4)",
          "border-bottom": "1px solid var(--color-border-default)",
        }}
      >
        <button
          onClick={goBack}
          aria-label="Back"
          style={{ "font-size": "var(--text-lg)", "min-height": "48px", "min-width": "48px" }}
        >
          ←
        </button>
        <h1 style={{ "font-size": "var(--text-xl)", "font-weight": "var(--font-semibold)" }}>
          Profile
        </h1>
      </header>

      <div style={{ padding: "var(--space-6)", display: "flex", "flex-direction": "column", "align-items": "center", gap: "var(--space-4)" }}>
        <Avatar name={user()?.display_name ?? "?"} size="lg" />
        <div style={{ "text-align": "center" }}>
          <div style={{ "font-size": "var(--text-xl)", "font-weight": "var(--font-semibold)" }}>
            {user()?.display_name}
          </div>
          <div style={{ "font-size": "var(--text-sm)", color: "var(--color-text-tertiary)" }}>
            @{user()?.username}
          </div>
          <div style={{ "font-size": "var(--text-sm)", color: "var(--color-text-tertiary)" }}>
            {user()?.email}
          </div>
        </div>
      </div>

      <div style={{ padding: "var(--space-4)", display: "flex", "flex-direction": "column", gap: "var(--space-5)" }}>
        {/* Settings */}
        <div>
          <h2 style={{ "font-size": "var(--text-lg)", "font-weight": "var(--font-semibold)", "margin-bottom": "var(--space-3)" }}>
            Settings
          </h2>
          <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-4)" }}>
            <div>
              <Input
                label="Auth URL"
                value={serverUrl()}
                onInput={setServerUrlLocal}
              />
              <Input
                label="Signaling URL"
                value={signalingUrlVal()}
                onInput={setSignalingUrlLocal}
                style={{ "margin-top": "var(--space-2)" }}
              />
              <button
                onClick={handleSaveUrl}
                style={{
                  "margin-top": "var(--space-2)",
                  color: "var(--color-brand-primary)",
                  "font-size": "var(--text-sm)",
                  "font-weight": "var(--font-medium)",
                  "min-height": "auto",
                  "min-width": "auto",
                }}
              >
                Save
              </button>
            </div>
            <Toggle
              label="End-of-transmission chirp"
              checked={chirpEnabled()}
              onChange={setChirpEnabled}
            />
            <Toggle
              label="Haptic feedback"
              checked={hapticsEnabled()}
              onChange={setHapticsEnabled}
            />
          </div>
        </div>

        {/* About */}
        <div>
          <h2 style={{ "font-size": "var(--text-lg)", "font-weight": "var(--font-semibold)", "margin-bottom": "var(--space-2)" }}>
            About
          </h2>
          <p style={{ "font-size": "var(--text-sm)", color: "var(--color-text-secondary)" }}>
            WalkieTalk v1.0.0
          </p>
        </div>

        {/* Logout */}
        <Button variant="danger" onClick={handleLogout} fullWidth>
          Log out
        </Button>
      </div>

      <Toast />
    </div>
  );
};

export default Profile;
