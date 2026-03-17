import { type Component, createSignal } from "solid-js";
import { navigate, Screen } from "../router";
import { joinByCode } from "../stores/rooms";
import Button from "../components/Button";
import Input from "../components/Input";
import BottomSheet from "../components/BottomSheet";

const JoinByCode: Component = () => {
  const [code, setCode] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");

  const isValid = () => code().length === 8;

  const handleJoin = async () => {
    if (!isValid() || loading()) return;
    setLoading(true);
    setError("");

    const result = await joinByCode(code());
    setLoading(false);

    if (result.ok && result.room) {
      navigate(Screen.RoomView, {
        roomId: result.room.id,
        roomName: result.room.name,
      });
    } else {
      setError("Invalid or expired invite code.");
    }
  };

  return (
    <BottomSheet isOpen onClose={() => navigate(Screen.RoomList)} title="Join by code">
      <form
        style={{
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-4)",
          padding: "var(--space-4)",
        }}
        onSubmit={(e) => {
          e.preventDefault();
          handleJoin();
        }}
      >
        <Input
          label="Invite code"
          value={code()}
          onInput={(v) => setCode(v.toUpperCase())}
          placeholder="ABCD1234"
          disabled={loading()}
          error={error()}
          helper="Ask the room owner for an invite code."
          style={{
            "font-family": "var(--font-mono)",
            "letter-spacing": "0.25em",
            "text-transform": "uppercase",
          }}
        />

        <Button
          variant="primary"
          disabled={!isValid() || loading()}
          loading={loading()}
          type="submit"
          fullWidth
        >
          Join
        </Button>
      </form>
    </BottomSheet>
  );
};

export default JoinByCode;
