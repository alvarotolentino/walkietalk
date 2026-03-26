import { type Component, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { login } from "../stores/auth";
import { connect } from "../stores/connection";
import Button from "../components/Button";
import Input from "../components/Input";
import Toast, { showToast } from "../components/Toast";

const Login: Component = () => {
  const [email, setEmail] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [showPassword, setShowPassword] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");

  const isValid = () =>
    email().includes("@") && email().length > 3 && password().length >= 8;

  const handleLogin = async () => {
    if (!isValid() || loading()) return;
    setLoading(true);
    setError("");

    const result = await login(email(), password());
    setLoading(false);

    if (result.ok) {
      connect().catch(() => {});
      navigate(Screen.RoomList);
    } else if (result.error === "invalid_credentials") {
      setError("Invalid email or password.");
    } else if (result.error === "network") {
      showToast("Unable to connect. Check your network.", "error");
    } else {
      showToast("Something went wrong. Try again.", "error");
    }
  };

  return (
    <div
      class="screen"
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        padding: "var(--space-8) var(--space-6)",
        gap: "var(--space-6)",
      }}
    >
      <div
        style={{
          "margin-top": "var(--space-12)",
          color: "var(--color-brand-primary)",
        }}
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="11" rx="3"/><path d="M5 10a7 7 0 0 0 14 0"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
      </div>
      <h1
        style={{
          "font-size": "var(--text-xl)",
          "font-weight": "var(--font-semibold)",
        }}
      >
        Welcome back
      </h1>

      <form
        style={{
          width: "100%",
          "max-width": "400px",
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-4)",
        }}
        onSubmit={(e) => {
          e.preventDefault();
          handleLogin();
        }}
      >
        <Input
          label="Email"
          type="email"
          value={email()}
          onInput={setEmail}
          placeholder="you@example.com"
          disabled={loading()}
        />
        <div style={{ position: "relative" }}>
          <Input
            label="Password"
            type={showPassword() ? "text" : "password"}
            value={password()}
            onInput={setPassword}
            placeholder="••••••••"
            disabled={loading()}
            error={error()}
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
          Log in
        </Button>
      </form>

      <p style={{ "font-size": "var(--text-sm)", color: "var(--color-text-secondary)" }}>
        Don't have an account?{" "}
        <button
          onClick={() => navigate(Screen.Register)}
          style={{
            color: "var(--color-brand-primary)",
            "font-weight": "var(--font-medium)",
            "min-height": "auto",
            "min-width": "auto",
            display: "inline",
            "text-decoration": "underline",
          }}
        >
          Register
        </button>
      </p>

      <Toast />
    </div>
  );
};

export default Login;
