import { type Component, onMount, onCleanup, Show, For, createMemo } from "solid-js";
import { navigate, goBack, currentParams, Screen } from "../router";
import {
  members,
  floorHolder,
  setRoomState,
  addMember,
  removeMember,
  updatePresence,
  setFloorHolder,
  clearActiveRoom,
} from "../stores/activeRoom";
import { isTransmitting, isReceiving, sendLevel, recvLevel, floorTimeRemaining } from "../stores/audio";
import { connectionState } from "../stores/connection";
import PttButton from "../components/PttButton";
import FloorBanner from "../components/FloorBanner";
import ConnectionBar from "../components/ConnectionBar";
import MemberList from "../components/MemberList";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { joinRoomWs, leaveRoomWs } from "../stores/activeRoom";

const RoomView: Component = () => {
  const params = currentParams();
  const roomId = () => params?.roomId ?? "";
  const roomName = () => params?.roomName ?? "Room";

  onMount(async () => {
    if (roomId()) {
      await joinRoomWs(roomId());
    }
  });

  onCleanup(async () => {
    if (roomId()) {
      await leaveRoomWs(roomId());
      clearActiveRoom();
    }
  });

  // Listen for Tauri events
  useTauriEvent("room_state", (data: any) => setRoomState(data));
  useTauriEvent("member_joined", (data: any) => addMember(data));
  useTauriEvent("member_left", (data: any) => removeMember(data.user_id));
  useTauriEvent("presence_update", (data: any) =>
    updatePresence(data.user_id, data.status)
  );
  useTauriEvent("floor_granted", () => setFloorHolder("self"));
  useTauriEvent("floor_occupied", (data: any) => setFloorHolder(data.user_id, data.display_name));
  useTauriEvent("floor_released", () => setFloorHolder(null));
  useTauriEvent("floor_denied", () => {});
  useTauriEvent("floor_timeout", () => setFloorHolder(null));

  const currentSpeaker = createMemo(() => {
    const holder = floorHolder();
    if (!holder) return null;
    if (holder.userId === "self") return { name: "You", isSelf: true };
    return { name: holder.displayName ?? "Someone", isSelf: false };
  });

  const handleBack = async () => {
    if (roomId()) {
      await leaveRoomWs(roomId());
      clearActiveRoom();
    }
    goBack();
  };

  return (
    <div class="screen" style={{ display: "flex", "flex-direction": "column" }}>
      {/* Header */}
      <header
        style={{
          display: "flex",
          "align-items": "center",
          gap: "var(--space-3)",
          padding: "var(--space-3) var(--space-4)",
          "border-bottom": "1px solid var(--color-border-default)",
        }}
      >
        <button
          onClick={handleBack}
          aria-label="Leave room and go back"
          style={{ "font-size": "var(--text-lg)", "min-height": "48px", "min-width": "48px" }}
        >
          ←
        </button>
        <h1
          style={{
            flex: "1",
            "font-size": "var(--text-lg)",
            "font-weight": "var(--font-semibold)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {roomName()}
        </h1>
        <button
          onClick={() =>
            navigate(Screen.RoomSettings, {
              roomId: roomId(),
              roomName: roomName(),
            })
          }
          aria-label="Room settings"
          style={{ "font-size": "var(--text-lg)", "min-height": "48px", "min-width": "48px" }}
        >
          ⚙
        </button>
      </header>

      {/* Connection bar */}
      <ConnectionBar />

      {/* Floor banner */}
      <Show when={currentSpeaker()}>
        {(speaker) => (
          <FloorBanner
            speakerName={speaker().name}
            isSelf={speaker().isSelf}
            level={speaker().isSelf ? sendLevel() : recvLevel()}
            timeRemaining={floorTimeRemaining()}
          />
        )}
      </Show>

      {/* Members */}
      <div class="scrollable" style={{ flex: "1", padding: "var(--space-4)" }}>
        <MemberList members={members()} floorHolderId={floorHolder()?.userId ?? null} />
      </div>

      {/* SR live region for floor events */}
      <div aria-live="assertive" class="sr-only" id="floor-announcements" />
      <div aria-live="polite" class="sr-only" id="member-announcements" />

      {/* PTT button */}
      <div
        style={{
          display: "flex",
          "flex-direction": "column",
          "align-items": "center",
          padding: "var(--space-4)",
          "padding-bottom": "calc(var(--space-8) + env(safe-area-inset-bottom, 0px))",
        }}
      >
        <PttButton
          roomId={roomId()}
          speakerName={currentSpeaker()?.name}
          isConnected={connectionState() === "connected"}
        />
      </div>
    </div>
  );
};

export default RoomView;
