import { type Component, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { createRoom } from "../stores/rooms";
import Button from "../components/Button";
import Input from "../components/Input";
import BottomSheet from "../components/BottomSheet";
import { showToast } from "../components/Toast";

const CreateRoom: Component = () => {
  const [name, setName] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const isValid = () => name().length >= 1 && name().length <= 128;

  const handleCreate = async () => {
    if (!isValid() || loading()) return;
    setLoading(true);

    const result = await createRoom(name(), description() || undefined);
    setLoading(false);

    if (result.ok && result.room) {
      navigate(Screen.RoomView, {
        roomId: result.room.id,
        roomName: result.room.name,
      });
    } else {
      showToast("Failed to create room.", "error");
    }
  };

  return (
    <BottomSheet isOpen onClose={() => navigate(Screen.RoomList)} title="Create room">
      <form
        style={{
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-4)",
          padding: "var(--space-4)",
        }}
        onSubmit={(e) => {
          e.preventDefault();
          handleCreate();
        }}
      >
        <Input
          label="Room name"
          value={name()}
          onInput={setName}
          placeholder="My room"
          disabled={loading()}
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
            Description (optional)
          </label>
          <textarea
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="What's this room about?"
            autocomplete="off"
            maxLength={500}
            disabled={loading()}
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
              "font-family": "var(--font-family)",
            }}
          />
        </div>

        <Button
          variant="primary"
          disabled={!isValid() || loading()}
          loading={loading()}
          type="submit"
          fullWidth
        >
          Create Room
        </Button>
      </form>
    </BottomSheet>
  );
};

export default CreateRoom;
