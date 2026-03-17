import { createSignal } from "solid-js";

const DEFAULT_SERVER_URL = "http://localhost:3000";

const [serverUrl, setServerUrlSignal] = createSignal(
  typeof localStorage !== "undefined"
    ? localStorage.getItem("walkietalk_server_url") ?? DEFAULT_SERVER_URL
    : DEFAULT_SERVER_URL
);

export function getServerUrl(): string {
  return serverUrl();
}

export function setServerUrl(url: string) {
  setServerUrlSignal(url);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem("walkietalk_server_url", url);
  }
}
