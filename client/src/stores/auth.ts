import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface User {
  id: string;
  username: string;
  email: string;
  display_name: string;
}

export interface AuthResult {
  ok: boolean;
  error?: string;
}

const [user, setUser] = createSignal<User | null>(null);
const [isAuthenticated, setIsAuthenticated] = createSignal(false);

export { user, isAuthenticated };

export async function checkAuth(): Promise<boolean> {
  try {
    const u = await invoke<User>("get_current_user");
    setUser(u);
    setIsAuthenticated(true);
    return true;
  } catch {
    setUser(null);
    setIsAuthenticated(false);
    return false;
  }
}

export async function login(email: string, password: string): Promise<AuthResult> {
  try {
    const u = await invoke<User>("login", { email, password });
    setUser(u);
    setIsAuthenticated(true);
    return { ok: true };
  } catch (e: unknown) {
    const msg = typeof e === "string" ? e : String(e);
    if (msg.includes("invalid_credentials")) return { ok: false, error: "invalid_credentials" };
    if (msg.includes("Network")) return { ok: false, error: "network" };
    return { ok: false, error: msg };
  }
}

export async function register(
  displayName: string,
  username: string,
  email: string,
  password: string,
): Promise<AuthResult> {
  try {
    const u = await invoke<User>("register", { displayName, username, email, password });
    setUser(u);
    setIsAuthenticated(true);
    return { ok: true };
  } catch (e: unknown) {
    const msg = typeof e === "string" ? e : String(e);
    if (msg.toLowerCase().includes("email")) return { ok: false, error: "email_taken" };
    if (msg.toLowerCase().includes("username")) return { ok: false, error: "username_taken" };
    if (msg.includes("Network")) return { ok: false, error: "network" };
    return { ok: false, error: msg };
  }
}

export async function logout(): Promise<void> {
  try {
    await invoke("logout");
  } finally {
    setUser(null);
    setIsAuthenticated(false);
  }
}
