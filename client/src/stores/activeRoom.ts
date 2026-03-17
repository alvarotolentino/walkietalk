import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface ActiveMember {
  user_id: string;
  display_name: string;
  status: "online" | "speaking" | "offline";
}

const [members, setMembers] = createSignal<ActiveMember[]>([]);
const [floorHolder, setFloorHolder] = createSignal<string | null>(null);
const [floorHolderName, setFloorHolderName] = createSignal<string | null>(null);

export { members, floorHolder, floorHolderName };

export function setRoomState(
  memberList: ActiveMember[],
  holder: string | null,
) {
  setMembers(memberList);
  setFloorHolder(holder);
  if (holder) {
    const m = memberList.find((m) => m.user_id === holder);
    setFloorHolderName(m?.display_name ?? null);
  } else {
    setFloorHolderName(null);
  }
}

export function addMember(member: ActiveMember) {
  setMembers((prev) => {
    if (prev.some((m) => m.user_id === member.user_id)) return prev;
    return [...prev, member];
  });
}

export function removeMember(userId: string) {
  setMembers((prev) => prev.filter((m) => m.user_id !== userId));
  if (floorHolder() === userId) {
    setFloorHolder(null);
    setFloorHolderName(null);
  }
}

export function updatePresence(userId: string, status: "online" | "speaking" | "offline") {
  setMembers((prev) =>
    prev.map((m) => (m.user_id === userId ? { ...m, status } : m))
  );
}

export function setFloorHolderState(userId: string | null, displayName: string | null) {
  setFloorHolder(userId);
  setFloorHolderName(displayName);
  if (userId) {
    updatePresence(userId, "speaking");
  }
}

export function clearActiveRoom() {
  setMembers([]);
  setFloorHolder(null);
  setFloorHolderName(null);
}

export async function joinRoomWs(roomId: string): Promise<void> {
  await invoke("join_room_ws", { roomId });
}

export async function leaveRoomWs(roomId: string): Promise<void> {
  await invoke("leave_room_ws", { roomId });
  clearActiveRoom();
}
