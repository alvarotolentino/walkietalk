import { type Component, createSignal, onMount, For, Show, createMemo } from "solid-js";
import { navigate, Screen } from "../router";
import { rooms, fetchRooms, leaveRoom } from "../stores/rooms";
import { user } from "../stores/auth";
import { connectionState, connect } from "../stores/connection";
import Avatar from "../components/Avatar";
import Badge from "../components/Badge";
import EmptyState from "../components/EmptyState";
import Toast, { showToast } from "../components/Toast";

const RoomList: Component = () => {
  const [search, setSearch] = createSignal("");
  const [refreshing, setRefreshing] = createSignal(false);
  const [showFab, setShowFab] = createSignal(false);

  onMount(async () => {
    await fetchRooms();
    if (connectionState() === "disconnected") {
      await connect();
    }
  });

  const filteredRooms = createMemo(() => {
    const q = search().toLowerCase();
    const list = rooms();
    if (!q) return list;
    return list.filter((r) => r.name.toLowerCase().includes(q));
  });

  const handleRefresh = async () => {
    setRefreshing(true);
    await fetchRooms();
    setRefreshing(false);
  };

  const handleLeave = async (roomId: string, roomName: string) => {
    const result = await leaveRoom(roomId);
    if (result.ok) {
      showToast(`Left ${roomName}`, "info");
    } else {
      showToast("Failed to leave room.", "error");
    }
  };

  return (
    <div class="screen" style={{ display: "flex", "flex-direction": "column" }}>
      {/* Header */}
      <header
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "var(--space-4) var(--space-4)",
          "border-bottom": "1px solid var(--color-border-default)",
        }}
      >
        <h1
          style={{ "font-size": "var(--text-xl)", "font-weight": "var(--font-bold)" }}
        >
          WalkieTalk
        </h1>
        <button
          onClick={() => navigate(Screen.Profile)}
          aria-label="Profile"
          style={{ "min-height": "48px", "min-width": "48px" }}
        >
          <Avatar name={user()?.display_name ?? "?"} size="sm" />
        </button>
      </header>

      {/* Search */}
      <div style={{ padding: "var(--space-3) var(--space-4)" }}>
        <input
          type="search"
          placeholder="Search rooms..."
          value={search()}
          onInput={(e) => setSearch(e.currentTarget.value)}
          aria-label="Search rooms"
          style={{
            width: "100%",
            padding: "var(--space-3) var(--space-4)",
            background: "var(--color-bg-tertiary)",
            border: "1px solid var(--color-border-default)",
            "border-radius": "var(--radius-md)",
            color: "var(--color-text-primary)",
            "font-size": "var(--text-base)",
            outline: "none",
          }}
        />
      </div>

      {/* Room list */}
      <div class="scrollable" style={{ flex: "1" }}>
        <Show
          when={filteredRooms().length > 0}
          fallback={
            <EmptyState
              title="No rooms yet"
              description="Create a room or join one to get started."
              action={
                <div style={{ display: "flex", gap: "var(--space-3)", "flex-wrap": "wrap", "justify-content": "center" }}>
                  <button
                    onClick={() => navigate(Screen.CreateRoom)}
                    style={{
                      padding: "var(--space-3) var(--space-5)",
                      background: "var(--color-brand-primary)",
                      color: "var(--color-text-primary)",
                      "border-radius": "var(--radius-md)",
                      "font-weight": "var(--font-medium)",
                    }}
                  >
                    Create Room
                  </button>
                  <button
                    onClick={() => navigate(Screen.JoinByCode)}
                    style={{
                      padding: "var(--space-3) var(--space-5)",
                      background: "var(--color-bg-tertiary)",
                      color: "var(--color-text-primary)",
                      "border-radius": "var(--radius-md)",
                      "font-weight": "var(--font-medium)",
                    }}
                  >
                    Join by Code
                  </button>
                </div>
              }
            />
          }
        >
          <ul role="list">
            <For each={filteredRooms()}>
              {(room) => (
                <li
                  role="listitem"
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "var(--space-4)",
                    "border-bottom": "1px solid var(--color-border-default)",
                    cursor: "pointer",
                    gap: "var(--space-3)",
                  }}
                  onClick={() =>
                    navigate(Screen.RoomView, {
                      roomId: room.id,
                      roomName: room.name,
                    })
                  }
                  aria-label={`${room.name}, ${room.member_count} members, ${room.visibility}`}
                >
                  <div style={{ flex: "1", "min-width": "0" }}>
                    <div
                      style={{
                        "font-weight": "var(--font-semibold)",
                        "font-size": "var(--text-lg)",
                        overflow: "hidden",
                        "text-overflow": "ellipsis",
                        "white-space": "nowrap",
                      }}
                    >
                      {room.name}
                    </div>
                    <div
                      style={{
                        "font-size": "var(--text-sm)",
                        color: "var(--color-text-secondary)",
                        "margin-top": "var(--space-1)",
                      }}
                    >
                      {room.visibility === "private" ? "🔒" : "🌐"}{" "}
                      {room.member_count} member{room.member_count !== 1 ? "s" : ""}
                    </div>
                  </div>
                  <Badge text={String(room.member_count)} />
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>

      {/* Pull to refresh hint */}
      <Show when={refreshing()}>
        <div
          style={{
            "text-align": "center",
            padding: "var(--space-2)",
            color: "var(--color-text-secondary)",
            "font-size": "var(--text-sm)",
          }}
        >
          Refreshing...
        </div>
      </Show>

      {/* FAB */}
      <button
        onClick={() => setShowFab(!showFab())}
        aria-label="Create or join a room"
        style={{
          position: "fixed",
          bottom: "calc(var(--space-8) + env(safe-area-inset-bottom, 0px))",
          right: "var(--space-6)",
          width: "56px",
          height: "56px",
          "border-radius": "var(--radius-full)",
          background: "var(--color-brand-primary)",
          color: "white",
          "font-size": "var(--text-2xl)",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "box-shadow": "var(--shadow-lg)",
          "z-index": "10",
        }}
      >
        +
      </button>

      {/* FAB action sheet */}
      <Show when={showFab()}>
        <div
          style={{
            position: "fixed",
            inset: "0",
            background: "var(--color-bg-overlay)",
            "z-index": "9",
          }}
          onClick={() => setShowFab(false)}
        />
        <div
          style={{
            position: "fixed",
            bottom: "calc(var(--space-20) + env(safe-area-inset-bottom, 0px))",
            right: "var(--space-6)",
            display: "flex",
            "flex-direction": "column",
            gap: "var(--space-2)",
            "z-index": "11",
          }}
        >
          {[
            { label: "Create Room", screen: Screen.CreateRoom },
            { label: "Join by Code", screen: Screen.JoinByCode },
            { label: "Browse Public", screen: Screen.PublicRooms },
          ].map((item) => (
            <button
              onClick={() => {
                setShowFab(false);
                navigate(item.screen);
              }}
              style={{
                padding: "var(--space-3) var(--space-5)",
                background: "var(--color-bg-secondary)",
                color: "var(--color-text-primary)",
                "border-radius": "var(--radius-md)",
                "font-size": "var(--text-sm)",
                "font-weight": "var(--font-medium)",
                "box-shadow": "var(--shadow-md)",
                "text-align": "right",
                "white-space": "nowrap",
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      </Show>

      <Toast />
    </div>
  );
};

export default RoomList;
