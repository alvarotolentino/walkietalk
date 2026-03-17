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

export interface RoomResult {
  ok: boolean;
  error?: string;
  room?: Room;
  rooms?: Room[];
  invite_code?: string;
}

const [rooms, setRooms] = createSignal<Room[]>([]);

export { rooms };

export async function fetchRooms(): Promise<void> {
  try {
    const list = await invoke<Room[]>("get_rooms");
    setRooms(list);
  } catch {
    // silently keep stale list
  }
}

export async function createRoom(
  name: string,
  description: string | undefined,
  visibility: "public" | "private",
): Promise<RoomResult> {
  try {
    const room = await invoke<Room>("create_room", { name, description: description ?? "", visibility });
    setRooms((prev) => [...prev, room]);
    return { ok: true, room };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function joinByCode(code: string): Promise<RoomResult> {
  try {
    const room = await invoke<Room>("join_by_code", { code });
    setRooms((prev) => {
      if (prev.some((r) => r.id === room.id)) return prev;
      return [...prev, room];
    });
    return { ok: true, room };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function joinRoom(roomId: string): Promise<RoomResult> {
  try {
    const room = await invoke<Room>("join_room", { roomId });
    setRooms((prev) => {
      if (prev.some((r) => r.id === room.id)) return prev;
      return [...prev, room];
    });
    return { ok: true, room };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function leaveRoom(roomId: string): Promise<RoomResult> {
  try {
    await invoke("leave_room", { roomId });
    setRooms((prev) => prev.filter((r) => r.id !== roomId));
    return { ok: true };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function fetchPublicRooms(search: string): Promise<RoomResult> {
  try {
    const list = await invoke<Room[]>("get_public_rooms", { search });
    return { ok: true, rooms: list };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function getRoomSettings(roomId: string): Promise<RoomResult & { room?: RoomSettings }> {
  try {
    const settings = await invoke<RoomSettings>("get_room_settings", { roomId });
    return { ok: true, room: settings };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function updateRoom(
  roomId: string,
  changes: { name: string; description?: string; visibility: "public" | "private" },
): Promise<RoomResult> {
  try {
    const { name, description, visibility } = changes;
    await invoke("update_room", { roomId, name, description: description ?? "", visibility });
    setRooms((prev) =>
      prev.map((r) => (r.id === roomId ? { ...r, name, description, visibility } : r))
    );
    return { ok: true };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function deleteRoom(roomId: string): Promise<RoomResult> {
  try {
    await invoke("delete_room", { roomId });
    setRooms((prev) => prev.filter((r) => r.id !== roomId));
    return { ok: true };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}

export async function regenerateInvite(roomId: string): Promise<RoomResult> {
  try {
    const code = await invoke<string>("regenerate_invite", { roomId });
    return { ok: true, invite_code: code };
  } catch (e: unknown) {
    return { ok: false, error: String(e) };
  }
}
