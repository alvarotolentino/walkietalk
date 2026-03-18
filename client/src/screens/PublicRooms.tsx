import { type Component, createSignal, onMount, For, Show } from "solid-js";
import { navigate, goBack, Screen } from "../router";
import { fetchPublicRooms, joinRoom, type Room } from "../stores/rooms";
import Button from "../components/Button";
import Badge from "../components/Badge";
import Toast, { showToast } from "../components/Toast";

const PublicRooms: Component = () => {
  const [search, setSearch] = createSignal("");
  const [roomList, setRoomList] = createSignal<Room[]>([]);
  const [loading, setLoading] = createSignal(false);
  let debounceTimer: number | undefined;

  const doSearch = async (query: string) => {
    setLoading(true);
    const result = await fetchPublicRooms(query);
    if (result.ok) {
      setRoomList(result.rooms ?? []);
    }
    setLoading(false);
  };

  onMount(() => doSearch(""));

  const handleSearchInput = (value: string) => {
    setSearch(value);
    clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => doSearch(value), 300);
  };

  const handleJoin = async (roomId: string, roomName: string) => {
    const result = await joinRoom(roomId);
    if (result.ok) {
      navigate(Screen.RoomView, { roomId, roomName });
    } else {
      showToast("Failed to join room.", "error");
    }
  };

  return (
    <div class="screen" style={{ display: "flex", "flex-direction": "column" }}>
      <header
        style={{
          display: "flex",
          "align-items": "center",
          gap: "var(--space-3)",
          padding: "var(--space-4)",
          "border-bottom": "1px solid var(--color-border-default)",
        }}
      >
        <button
          onClick={goBack}
          aria-label="Back"
          style={{ "font-size": "var(--text-lg)", "min-height": "48px", "min-width": "48px" }}
        >
          ←
        </button>
        <h1 style={{ "font-size": "var(--text-xl)", "font-weight": "var(--font-semibold)" }}>
          Public Rooms
        </h1>
      </header>

      <div style={{ padding: "var(--space-3) var(--space-4)" }}>
        <input
          type="search"
          placeholder="Search public rooms..."
          value={search()}
          onInput={(e) => handleSearchInput(e.currentTarget.value)}
          aria-label="Search public rooms"
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

      <div class="scrollable" style={{ flex: "1" }}>
        <Show
          when={roomList().length > 0}
          fallback={
            <div
              style={{
                "text-align": "center",
                padding: "var(--space-12)",
                color: "var(--color-text-secondary)",
              }}
            >
              {loading() ? "Searching..." : "No public rooms found."}
            </div>
          }
        >
          <ul role="list">
            <For each={roomList()}>
              {(room) => (
                <li
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "var(--space-4)",
                    "border-bottom": "1px solid var(--color-border-default)",
                    gap: "var(--space-3)",
                  }}
                >
                  <div style={{ flex: "1", "min-width": "0" }}>
                    <div
                      style={{
                        "font-weight": "var(--font-semibold)",
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
                      }}
                    >
                      {room.member_count} member{room.member_count !== 1 ? "s" : ""}
                    </div>
                  </div>
                  <button
                    onClick={() =>
                      room.is_member
                        ? navigate(Screen.RoomView, { roomId: room.id, roomName: room.name })
                        : handleJoin(room.id, room.name)
                    }
                    style={{
                      padding: "var(--space-2) var(--space-4)",
                      background: room.is_member
                        ? "var(--color-bg-tertiary)"
                        : "var(--color-brand-primary)",
                      color: "var(--color-text-primary)",
                      "border-radius": "var(--radius-md)",
                      "font-size": "var(--text-sm)",
                      "font-weight": "var(--font-medium)",
                    }}
                  >
                    {room.is_member ? "Open" : "Join"}
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>

      <Toast />
    </div>
  );
};

export default PublicRooms;
