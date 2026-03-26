import { type Component, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { register } from "../stores/auth";
import { connect } from "../stores/connection";
import Button from "../components/Button";
import Input from "../components/Input";
import Toast, { showToast } from "../components/Toast";

const Register: Component = () => {
  const [displayName, setDisplayName] = createSignal("");
  const [username, setUsername] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [showPassword, setShowPassword] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [emailError, setEmailError] = createSignal("");
  const [usernameError, setUsernameError] = createSignal("");

  const isValid = () =>
    displayName().length >= 1 &&
    displayName().length <= 64 &&
    username().length >= 3 &&
    username().length <= 32 &&
    /^[a-zA-Z0-9-]+$/.test(username()) &&
    email().includes("@") &&
    email().length > 3 &&
    password().length >= 8;

  const handleRegister = async () => {
    if (!isValid() || loading()) return;
    setLoading(true);
    setEmailError("");
    setUsernameError("");

    const result = await register(
      displayName(),
      username(),
      email(),
      password()
    );
    setLoading(false);

    if (result.ok) {
      connect().catch(() => {});
      navigate(Screen.RoomList);
    } else if (result.error === "email_taken") {
      setEmailError("This email is already registered.");
    } else if (result.error === "username_taken") {
      setUsernameError("This username is taken.");
    } else if (result.error === "network") {
      showToast("Unable to connect. Check your network.", "error");
    } else {
      showToast("Something went wrong. Try again.", "error");
    }
  };

  return (
    <div
      class="screen scrollable"
      style={{
        display: "flex",
        "flex-direction": "column",
        padding: "var(--space-6)",
        gap: "var(--space-5)",
      }}
    >
      <button
        onClick={() => navigate(Screen.Login)}
        aria-label="Back to login"
        style={{
          "align-self": "flex-start",
          color: "var(--color-text-secondary)",
          "font-size": "var(--text-lg)",
          "min-height": "48px",
          "min-width": "48px",
        }}
      >
        ← Back
      </button>

      <h1
        style={{
          "font-size": "var(--text-xl)",
          "font-weight": "var(--font-semibold)",
        }}
      >
        Create account
      </h1>

      <form
        style={{
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-4)",
        }}
        onSubmit={(e) => {
          e.preventDefault();
          handleRegister();
        }}
      >
        <Input
          label="Display name"
          value={displayName()}
          onInput={setDisplayName}
          placeholder="John Doe"
          disabled={loading()}
        />
        <Input
          label="Username"
          value={username()}
          onInput={setUsername}
          placeholder="johndoe"
          disabled={loading()}
          error={usernameError()}
          helper="3–32 characters, letters, numbers, hyphens."
        />
        <Input
          label="Email"
          type="email"
          value={email()}
          onInput={setEmail}
          placeholder="you@example.com"
          disabled={loading()}
          error={emailError()}
        />
        <div style={{ position: "relative" }}>
          <Input
            label="Password"
            type={showPassword() ? "text" : "password"}
            value={password()}
            onInput={setPassword}
            placeholder="••••••••"
            disabled={loading()}
            helper="At least 8 characters."
          />
          <button
            type="button"
            onClick={() => setShowPassword(!showPassword())}
            aria-label={showPassword() ? "Hide password" : "Show password"}
            style={{
              position: "absolute",
              right: "var(--space-3)",
              top: "34px",
              color: "var(--color-text-secondary)",
              "font-size": "var(--text-sm)",
              "min-height": "32px",
              "min-width": "32px",
            }}
          >
            {showPassword() ? "Hide" : "Show"}
          </button>
        </div>

        <Button
          variant="primary"
          disabled={!isValid() || loading()}
          loading={loading()}
          type="submit"
          fullWidth
        >
          Create account
        </Button>
      </form>

      <Toast />
    </div>
  );
};

export default Register;
