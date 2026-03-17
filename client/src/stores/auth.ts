import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface User {
  id: string;
  username: string;
  email: string;
  display_name: string;
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

export async function login(email: string, password: string): Promise<void> {
  const u = await invoke<User>("login", { email, password });
  setUser(u);
  setIsAuthenticated(true);
}

export async function register(
  displayName: string,
  username: string,
  email: string,
  password: string,
): Promise<void> {
  const u = await invoke<User>("register", { displayName, username, email, password });
  setUser(u);
  setIsAuthenticated(true);
}

export async function logout(): Promise<void> {
  try {
    await invoke("logout");
  } finally {
    setUser(null);
    setIsAuthenticated(false);
  }
}
