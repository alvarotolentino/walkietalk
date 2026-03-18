import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ConnectionState = "connected" | "connecting" | "reconnecting" | "disconnected" | "failed";

const [connectionState, setConnectionState] = createSignal<ConnectionState>("disconnected");
const [reconnectAttempt, setReconnectAttempt] = createSignal(0);

export { connectionState, reconnectAttempt };

// Listen for connection_state events from the Rust backend
let listenersInitialized = false;
function initListeners() {
  if (listenersInitialized) return;
  listenersInitialized = true;

  listen<string>("connection_state", (event) => {
    const payload = event.payload;
    if (payload === "connected") {
      setConnectionState("connected");
      setReconnectAttempt(0);
    } else if (typeof payload === "string" && payload.startsWith("{")) {
      try {
        const data = JSON.parse(payload);
        if (data.state === "disconnected" && data.will_reconnect) {
          setConnectionState("reconnecting");
        } else if (data.state === "failed") {
          setConnectionState("failed");
        } else {
          setConnectionState("disconnected");
        }
      } catch {
        setConnectionState("disconnected");
      }
    } else {
      setConnectionState("disconnected");
    }
  });

  listen<string>("reconnecting", (event) => {
    setConnectionState("reconnecting");
    try {
      const data = JSON.parse(event.payload);
      setReconnectAttempt(data.attempt ?? 0);
    } catch {
      // ignore
    }
  });
}

export function updateConnectionState(state: ConnectionState, attempt?: number) {
  setConnectionState(state);
  if (attempt !== undefined) setReconnectAttempt(attempt);
  if (state === "connected") setReconnectAttempt(0);
}

export async function connect(): Promise<void> {
  initListeners();
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
    setReconnectAttempt(0);
  }
}

export async function reconnect(): Promise<void> {
  initListeners();
  setConnectionState("reconnecting");
  try {
    await invoke("reconnect");
    setConnectionState("connected");
    setReconnectAttempt(0);
  } catch {
    setConnectionState("disconnected");
  }
}
