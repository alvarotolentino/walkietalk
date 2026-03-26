import { createSignal } from "solid-js";

export enum Screen {
  Splash = "Splash",
  Login = "Login",
  Register = "Register",
  RoomList = "RoomList",
  CreateRoom = "CreateRoom",
  JoinByCode = "JoinByCode",
  RoomView = "RoomView",
  RoomSettings = "RoomSettings",
  Profile = "Profile",
}

export interface RouteParams {
  roomId?: string;
  roomName?: string;
}

interface HistoryEntry {
  screen: Screen;
  params?: RouteParams;
}

const [history, setHistory] = createSignal<HistoryEntry[]>([
  { screen: Screen.Splash },
]);

export function currentScreen(): Screen {
  const h = history();
  return h[h.length - 1].screen;
}

export function currentParams(): RouteParams | undefined {
  const h = history();
  return h[h.length - 1].params;
}

export function navigate(screen: Screen, params?: RouteParams): void {
  setHistory((prev) => [...prev, { screen, params }]);
}

export function goBack(): void {
  setHistory((prev) => (prev.length > 1 ? prev.slice(0, -1) : prev));
}

export function resetTo(screen: Screen, params?: RouteParams): void {
  setHistory([{ screen, params }]);
}
