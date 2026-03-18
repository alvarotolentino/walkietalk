import { type Component, onMount, onCleanup, Show, createMemo } from "solid-js";
import { navigate, goBack, currentParams, Screen } from "../router";
import {
  members,
  floorHolder,
  floorHolderName,
  setRoomState,
  addMember,
  removeMember,
  updatePresence,
  setFloorHolderState,
  clearActiveRoom,
  joinRoomWs,
  leaveRoomWs,
} from "../stores/activeRoom";
import { user } from "../stores/auth";
import { isTransmitting, isReceiving, sendLevel, recvLevel, floorTimeRemaining } from "../stores/audio";
import { connectionState } from "../stores/connection";
import PttButton from "../components/PttButton";
import FloorBanner from "../components/FloorBanner";
import ConnectionBar from "../components/ConnectionBar";
import MemberList from "../components/MemberList";
import { useTauriEvent } from "../hooks/useTauriEvent";

const RoomView: Component = () => {
  const params = currentParams();
  const roomId = () => params?.roomId ?? "";
  const roomName = () => params?.roomName ?? "Room";
  const myUserId = () => user()?.id ?? "";

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

  // Listen for Tauri events — payloads are parsed JSON objects from dispatch_text
  useTauriEvent("room_state", (data: any) => {
    const memberList = (data.members ?? []).map((m: any) => ({
      user_id: m.user_id,
      display_name: m.display_name,
      status: m.status?.toLowerCase() ?? "online",
    }));
    const holder = data.floor_holder ?? null;
    setRoomState(memberList, holder);
  });

  useTauriEvent("member_joined", (data: any) => {
    if (data.user) {
      addMember({
        user_id: data.user.user_id,
        display_name: data.user.display_name,
        status: (data.user.status ?? "online").toLowerCase(),
      });
    }
  });

  useTauriEvent("member_left", (data: any) => removeMember(data.user_id));

  useTauriEvent("presence_update", (data: any) =>
    updatePresence(data.user_id, (data.status ?? "online").toLowerCase())
  );

  useTauriEvent("floor_granted", (data: any) => {
    // If the granted user_id matches our own, we are the speaker
    const granted = data.user_id;
    if (granted === myUserId()) {
      setFloorHolderState(granted, "You");
    } else {
      // Another user was granted — shouldn't normally happen via this event
      setFloorHolderState(granted, null);
    }
  });

  useTauriEvent("floor_occupied", (data: any) => {
    setFloorHolderState(data.speaker_id, data.display_name);
  });

  useTauriEvent("floor_released", () => setFloorHolderState(null, null));
  useTauriEvent("floor_denied", () => {});
  useTauriEvent("floor_timeout", () => setFloorHolderState(null, null));

  const currentSpeaker = createMemo(() => {
    const holderId = floorHolder();
    if (!holderId) return null;
    const isSelf = holderId === myUserId();
    const name = isSelf ? "You" : (floorHolderName() ?? "Someone");
    return { name, isSelf };
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
        <MemberList members={members()} floorHolderId={floorHolder() ?? undefined} />
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
