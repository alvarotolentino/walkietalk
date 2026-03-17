import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface Room {
  id: string;
  name: string;
  description?: string;
  visibility: "public" | "private";
  member_count: number;
  owner_id: string;
  invite_code?: string;
}

export interface RoomSettings {
  id: string;
  name: string;
  description: string;
  visibility: "public" | "private";
  owner_id: string;
  member_count: number;
  invite_code?: string;
  members: RoomMember[];
}

export interface RoomMember {
  user_id: string;
  display_name: string;
  role: "owner" | "member";
}

const [rooms, setRooms] = createSignal<Room[]>([]);

export { rooms };

export async function fetchRooms(): Promise<void> {
  const list = await invoke<Room[]>("get_rooms");
  setRooms(list);
}

export async function createRoom(
  name: string,
  description: string,
  visibility: "public" | "private",
): Promise<Room> {
  const room = await invoke<Room>("create_room", { name, description, visibility });
  setRooms((prev) => [...prev, room]);
  return room;
}

export async function joinByCode(code: string): Promise<Room> {
  const room = await invoke<Room>("join_by_code", { code });
  setRooms((prev) => {
    if (prev.some((r) => r.id === room.id)) return prev;
    return [...prev, room];
  });
  return room;
}

export async function joinRoom(roomId: string): Promise<Room> {
  const room = await invoke<Room>("join_room", { roomId });
  setRooms((prev) => {
    if (prev.some((r) => r.id === room.id)) return prev;
    return [...prev, room];
  });
  return room;
}

export async function leaveRoom(roomId: string): Promise<void> {
  await invoke("leave_room", { roomId });
  setRooms((prev) => prev.filter((r) => r.id !== roomId));
}

export async function fetchPublicRooms(search: string): Promise<Room[]> {
  return invoke<Room[]>("get_public_rooms", { search });
}

export async function getRoomSettings(roomId: string): Promise<RoomSettings> {
  return invoke<RoomSettings>("get_room_settings", { roomId });
}

export async function updateRoom(
  roomId: string,
  name: string,
  description: string,
  visibility: "public" | "private",
): Promise<void> {
  await invoke("update_room", { roomId, name, description, visibility });
  setRooms((prev) =>
    prev.map((r) => (r.id === roomId ? { ...r, name, description, visibility } : r))
  );
}

export async function deleteRoom(roomId: string): Promise<void> {
  await invoke("delete_room", { roomId });
  setRooms((prev) => prev.filter((r) => r.id !== roomId));
}

export async function regenerateInvite(roomId: string): Promise<string> {
  return invoke<string>("regenerate_invite", { roomId });
}
