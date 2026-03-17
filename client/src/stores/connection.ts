import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export type ConnectionState = "connected" | "connecting" | "reconnecting" | "disconnected";

const [connectionState, setConnectionState] = createSignal<ConnectionState>("disconnected");
const [reconnectAttempt, setReconnectAttempt] = createSignal(0);

export { connectionState, reconnectAttempt };

export function updateConnectionState(state: ConnectionState, attempt?: number) {
  setConnectionState(state);
  if (attempt !== undefined) setReconnectAttempt(attempt);
  if (state === "connected") setReconnectAttempt(0);
}

export async function connect(): Promise<void> {
  setConnectionState("connecting");
  try {
    await invoke("connect");
    setConnectionState("connected");
    setReconnectAttempt(0);
  } catch {
    setConnectionState("disconnected");
  }
}

export async function disconnect(): Promise<void> {
  try {
    await invoke("disconnect");
  } finally {
    setConnectionState("disconnected");
  }
}
