import { type Component, onMount, onCleanup, Show, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
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
import {
  sendLevel,
  recvLevel,
  floorTimeRemaining,
  startTransmitting,
  stopTransmitting,
  startReceiving,
  stopReceiving,
  updateSendLevel,
  updateRecvLevel,
  resetAudioState,
} from "../stores/audio";
import { connectionState } from "../stores/connection";
import PttButton from "../components/PttButton";
import FloorBanner from "../components/FloorBanner";
import ConnectionBar from "../components/ConnectionBar";
import MemberList from "../components/MemberList";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { triggerHaptic } from "../utils/haptics";

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
      invoke("stop_audio_capture").catch(() => {});
      await leaveRoomWs(roomId());
      clearActiveRoom();
      resetAudioState();
    }
  });

  // Helper: announce floor state changes to screen readers
  function announceFloor(message: string) {
    const el = document.getElementById("floor-announcements");
    if (el) el.textContent = message;
  }
  function announceMember(message: string) {
    const el = document.getElementById("member-announcements");
    if (el) el.textContent = message;
  }

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
      announceMember(`${data.user.display_name} joined the room`);
    }
  });

  useTauriEvent("member_left", (data: any) => {
    const member = members().find((m) => m.user_id === data.user_id);
    removeMember(data.user_id);
    if (member) announceMember(`${member.display_name} left the room`);
  });

  useTauriEvent("presence_update", (data: any) =>
    updatePresence(data.user_id, (data.status ?? "online").toLowerCase())
  );

  useTauriEvent("floor_granted", (data: any) => {
    const granted = data.user_id;
    const me = user();
    if (granted && me && granted === me.id) {
      setFloorHolderState(granted, "You");
      startTransmitting();
      triggerHaptic("heavy");
      announceFloor("Floor granted. You are now speaking.");
      // Start audio capture pipeline
      invoke("start_audio_capture", { roomId: roomId(), userId: me.id }).catch((e: unknown) =>
        console.error("start_audio_capture failed:", e)
      );
    } else {
      setFloorHolderState(granted, null);
    }
  });

  useTauriEvent("floor_occupied", (data: any) => {
    setFloorHolderState(data.speaker_id, data.display_name);
    const me = user();
    if (!me || data.speaker_id !== me.id) {
      startReceiving();
      triggerHaptic("rigid");
      announceFloor(`${data.display_name ?? "Someone"} is now speaking`);
    }
  });

  useTauriEvent("floor_released", (data: any) => {
    const wasMe = data.user_id && user()?.id === data.user_id;
    setFloorHolderState(null, null);
    if (wasMe) {
      stopTransmitting();
      triggerHaptic("light");
      announceFloor("Floor released. You stopped speaking.");
      invoke("stop_audio_capture").catch((e: unknown) =>
        console.error("stop_audio_capture failed:", e)
      );
    } else {
      stopReceiving();
      announceFloor("Floor is now free.");
    }
  });

  useTauriEvent("floor_denied", (data: any) => {
    triggerHaptic("error");
    const reason = data.reason
      ? `Floor denied: ${data.reason}`
      : "Floor denied. Someone else is speaking.";
    announceFloor(reason);
  });

  useTauriEvent("floor_timeout", (data: any) => {
    const wasMe = data.user_id && user()?.id === data.user_id;
    setFloorHolderState(null, null);
    if (wasMe) {
      stopTransmitting();
      triggerHaptic("error");
      announceFloor("Floor timed out. Your turn ended.");
      invoke("stop_audio_capture").catch((e: unknown) =>
        console.error("stop_audio_capture failed:", e)
      );
    } else {
      stopReceiving();
      announceFloor("Speaker timed out. Floor is now free.");
    }
  });

  // Audio level events from Tauri (mic/speaker RMS)
  useTauriEvent("audio_level", (data: any) => {
    if (data.direction === "send") {
      updateSendLevel(data.level ?? 0);
    } else if (data.direction === "recv") {
      updateRecvLevel(data.level ?? 0);
    }
  });

  const currentSpeaker = createMemo(() => {
    const holderId = floorHolder();
    if (!holderId) return null;
    const isSelf = holderId === myUserId();
    const name = isSelf ? "You" : (floorHolderName() ?? "Someone");
    return { name, isSelf };
  });

  const handleBack = async () => {
    if (roomId()) {
      invoke("stop_audio_capture").catch(() => {});
      await leaveRoomWs(roomId());
      clearActiveRoom();
      resetAudioState();
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
