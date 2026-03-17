import { onCleanup } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Subscribe to a Tauri event for the lifecycle of the current component.
 * Automatically unsubscribes on cleanup.
 */
export function useTauriEvent<T>(eventName: string, handler: (payload: T) => void) {
  let unlisten: UnlistenFn | undefined;

  listen<T>(eventName, (event) => {
    handler(event.payload);
  }).then((fn) => {
    unlisten = fn;
  });

  onCleanup(() => {
    unlisten?.();
  });
}
