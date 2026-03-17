import { type Component, createSignal, onMount, Show } from "solid-js";
import { goBack, currentParams } from "../router";
import { getRoomSettings, updateRoom, deleteRoom, regenerateInvite } from "../stores/rooms";
import Button from "../components/Button";
import Input from "../components/Input";
import Toggle from "../components/Toggle";
import Modal from "../components/Modal";
import Toast, { showToast } from "../components/Toast";
import { navigate, Screen } from "../router";
import { user } from "../stores/auth";
import MemberList from "../components/MemberList";

const RoomSettings: Component = () => {
  const params = currentParams();
  const roomId = () => params?.roomId ?? "";

  const [roomName, setRoomName] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [isPublic, setIsPublic] = createSignal(true);
  const [inviteCode, setInviteCode] = createSignal("");
  const [ownerId, setOwnerId] = createSignal("");
  const [memberList, setMemberList] = createSignal<any[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [showDeleteModal, setShowDeleteModal] = createSignal(false);
  const [showLeaveModal, setShowLeaveModal] = createSignal(false);

  const isOwner = () => ownerId() === user()?.id;

  onMount(async () => {
    const result = await getRoomSettings(roomId());
    if (result.ok && result.room) {
      setRoomName(result.room.name);
      setDescription(result.room.description ?? "");
      setIsPublic(result.room.visibility === "public");
      setInviteCode(result.room.invite_code ?? "");
      setOwnerId(result.room.owner_id);
      setMemberList(result.room.members ?? []);
    }
  });

  const handleSave = async () => {
    setLoading(true);
    const result = await updateRoom(roomId(), {
      name: roomName(),
      description: description() || undefined,
      visibility: isPublic() ? "public" : "private",
    });
    setLoading(false);
    if (result.ok) {
      showToast("Room updated.", "success");
    } else {
      showToast("Failed to update room.", "error");
    }
  };

  const handleRegenerate = async () => {
    const result = await regenerateInvite(roomId());
    if (result.ok && result.invite_code) {
      setInviteCode(result.invite_code);
      showToast("Invite code regenerated.", "success");
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(inviteCode());
    showToast("Copied to clipboard.", "info");
  };

  const handleDelete = async () => {
    const result = await deleteRoom(roomId());
    if (result.ok) {
      navigate(Screen.RoomList);
      showToast("Room deleted.", "info");
    } else {
      showToast("Failed to delete room.", "error");
    }
  };

  const handleLeave = async () => {
    const { leaveRoom } = await import("../stores/rooms");
    const result = await leaveRoom(roomId());
    if (result.ok) {
      navigate(Screen.RoomList);
    } else {
      showToast("Failed to leave room.", "error");
    }
  };

  return (
    <div class="screen scrollable" style={{ "flex-direction": "column" }}>
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
          Room Settings
        </h1>
      </header>

      <div style={{ padding: "var(--space-4)", display: "flex", "flex-direction": "column", gap: "var(--space-5)" }}>
        <Input
          label="Room name"
          value={roomName()}
          onInput={setRoomName}
          disabled={!isOwner() || loading()}
        />
        <div>
          <label
            style={{
              display: "block",
              "font-size": "var(--text-sm)",
              "font-weight": "var(--font-medium)",
              color: "var(--color-text-secondary)",
              "margin-bottom": "var(--space-1)",
            }}
          >
            Description
          </label>
          <textarea
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            disabled={!isOwner() || loading()}
            rows={3}
            style={{
              width: "100%",
              padding: "var(--space-3)",
              background: "var(--color-bg-tertiary)",
              border: "1px solid var(--color-border-default)",
              "border-radius": "var(--radius-md)",
              color: "var(--color-text-primary)",
              "font-size": "var(--text-base)",
              resize: "vertical",
              outline: "none",
            }}
          />
        </div>

        <Show when={isOwner()}>
          <Toggle
            label={isPublic() ? "Public" : "Private"}
            checked={isPublic()}
            onChange={setIsPublic}
          />
          <p style={{ "font-size": "var(--text-sm)", color: "var(--color-text-tertiary)" }}>
            {isPublic()
              ? "Anyone can find and join this room."
              : "Changing to public makes this room discoverable by anyone."}
          </p>

          <Button variant="primary" onClick={handleSave} loading={loading()} fullWidth>
            Save Changes
          </Button>
        </Show>

        {/* Invite section */}
        <Show when={!isPublic() && inviteCode()}>
          <div>
            <h2 style={{ "font-size": "var(--text-lg)", "font-weight": "var(--font-semibold)", "margin-bottom": "var(--space-3)" }}>
              Invite Code
            </h2>
            <div
              style={{
                "font-family": "var(--font-mono)",
                "font-size": "var(--text-2xl)",
                "letter-spacing": "0.2em",
                "text-align": "center",
                padding: "var(--space-4)",
                background: "var(--color-bg-tertiary)",
                "border-radius": "var(--radius-md)",
                "margin-bottom": "var(--space-3)",
              }}
            >
              {inviteCode()}
            </div>
            <div style={{ display: "flex", gap: "var(--space-2)" }}>
              <Button variant="secondary" onClick={handleCopy} fullWidth>
                Copy Code
              </Button>
              <Show when={isOwner()}>
                <Button variant="ghost" onClick={handleRegenerate} fullWidth>
                  Regenerate
                </Button>
              </Show>
            </div>
          </div>
        </Show>

        {/* Members */}
        <div>
          <h2 style={{ "font-size": "var(--text-lg)", "font-weight": "var(--font-semibold)", "margin-bottom": "var(--space-3)" }}>
            Members ({memberList().length}/500)
          </h2>
          <MemberList members={memberList()} floorHolderId={null} />
        </div>

        {/* Danger zone */}
        <div style={{ "margin-top": "var(--space-6)", "padding-top": "var(--space-4)", "border-top": "1px solid var(--color-border-default)" }}>
          <Button variant="danger" onClick={() => setShowLeaveModal(true)} fullWidth>
            Leave Room
          </Button>
          <Show when={isOwner()}>
            <div style={{ "margin-top": "var(--space-3)" }}>
              <Button variant="danger" onClick={() => setShowDeleteModal(true)} fullWidth>
                Delete Room
              </Button>
            </div>
          </Show>
        </div>
      </div>

      {/* Delete confirmation */}
      <Show when={showDeleteModal()}>
        <Modal
          title={`Delete ${roomName()}?`}
          message="This action cannot be undone."
          confirmLabel="Delete"
          onConfirm={handleDelete}
          onCancel={() => setShowDeleteModal(false)}
        />
      </Show>

      {/* Leave confirmation */}
      <Show when={showLeaveModal()}>
        <Modal
          title={`Leave ${roomName()}?`}
          confirmLabel="Leave"
          onConfirm={handleLeave}
          onCancel={() => setShowLeaveModal(false)}
        />
      </Show>

      <Toast />
    </div>
  );
};

export default RoomSettings;
