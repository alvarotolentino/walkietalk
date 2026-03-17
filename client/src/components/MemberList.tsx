import { type Component, For, createMemo, createSignal, Show } from "solid-js";
import MemberRow from "./MemberRow";
import { user } from "../stores/auth";

export interface MemberItem {
  user_id: string;
  display_name: string;
  status: "online" | "speaking" | "offline";
}

export interface MemberListProps {
  members: MemberItem[];
  floorHolderId?: string;
}

const MemberList: Component<MemberListProps> = (props) => {
  const autoCollapse = () => props.members.length > 10;
  const [collapsed, setCollapsed] = createSignal(false);

  // Sort: online/speaking first, then offline; within each group alphabetical
  const sorted = createMemo(() =>
    [...props.members].sort((a, b) => {
      const aOff = a.status === "offline" ? 1 : 0;
      const bOff = b.status === "offline" ? 1 : 0;
      if (aOff !== bOff) return aOff - bOff;
      return a.display_name.localeCompare(b.display_name);
    })
  );

  const displayed = createMemo(() =>
    collapsed() ? sorted().slice(0, 5) : sorted()
  );

  const me = () => user();

  return (
    <div>
      <button
        onClick={() => autoCollapse() && setCollapsed(!collapsed())}
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          width: "100%",
          padding: "var(--space-2) 0",
          background: "none",
          border: "none",
          color: "var(--color-text-secondary)",
          "font-size": "var(--text-sm)",
          "font-weight": "var(--font-semibold)",
          cursor: autoCollapse() ? "pointer" : "default",
          "min-height": "auto",
          "min-width": "auto",
        }}
        aria-expanded={!collapsed()}
        aria-controls="member-list"
      >
        <span>Members ({props.members.length})</span>
        <Show when={autoCollapse()}>
          <span aria-hidden="true">{collapsed() ? "▼" : "▲"}</span>
        </Show>
      </button>
      <ul
        id="member-list"
        role="list"
        style={{
          "list-style": "none",
          padding: "0",
          margin: "0",
        }}
      >
        <For each={displayed()}>
          {(member) => (
            <MemberRow
              displayName={member.display_name}
              status={member.status}
              isFloorHolder={member.user_id === props.floorHolderId}
              isSelf={member.user_id === me()?.id}
            />
          )}
        </For>
      </ul>
      <Show when={collapsed() && sorted().length > 5}>
        <button
          onClick={() => setCollapsed(false)}
          style={{
            "font-size": "var(--text-sm)",
            color: "var(--color-brand-primary)",
            background: "none",
            border: "none",
            cursor: "pointer",
            padding: "var(--space-2) 0",
            "min-height": "auto",
            "min-width": "auto",
          }}
        >
          Show all {sorted().length} members
        </button>
      </Show>
    </div>
  );
};

export default MemberList;
