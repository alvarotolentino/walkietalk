import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

const DEFAULT_SERVER_URL = "http://localhost:3000";

const [serverUrl, setServerUrlSignal] = createSignal(DEFAULT_SERVER_URL);

export function getServerUrl(): string {
  return serverUrl();
}

export async function loadServerUrl(): Promise<void> {
  try {
    const url = await invoke<string>("get_server_url");
    setServerUrlSignal(url);
  } catch {
    // Use default
  }
}

export async function setServerUrl(url: string): Promise<void> {
  setServerUrlSignal(url);
  try {
    await invoke("set_server_url", { url });
  } catch {
    // Persist failed — in-memory is still updated
  }
}
