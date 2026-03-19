import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

const DEFAULT_SERVER_URL = "http://localhost:3001";
const DEFAULT_SIGNALING_URL = "http://localhost:3002";

const [serverUrl, setServerUrlSignal] = createSignal(DEFAULT_SERVER_URL);
const [signalingUrl, setSignalingUrlSignal] = createSignal(DEFAULT_SIGNALING_URL);

export function getServerUrl(): string {
  return serverUrl();
}

export function getSignalingUrl(): string {
  return signalingUrl();
}

export async function loadServerUrl(): Promise<void> {
  try {
    const url = await invoke<string>("get_server_url");
    setServerUrlSignal(url);
  } catch {
    // Use default
  }
  try {
    const url = await invoke<string>("get_signaling_url");
    setSignalingUrlSignal(url);
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

export async function setSignalingUrl(url: string): Promise<void> {
  setSignalingUrlSignal(url);
  try {
    await invoke("set_signaling_url", { url });
  } catch {
    // Persist failed — in-memory is still updated
  }
}
