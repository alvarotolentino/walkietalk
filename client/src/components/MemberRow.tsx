import { type Component } from "solid-js";
import Avatar from "./Avatar";
import PresenceDot, { type PresenceState } from "./PresenceDot";

export interface MemberRowProps {
  displayName: string;
  status: PresenceState;
  isFloorHolder: boolean;
  isSelf: boolean;
}

const MemberRow: Component<MemberRowProps> = (props) => {
  const accessibleLabel = () => {
    const parts = [props.displayName];
    if (props.isSelf) parts.push("(you)");
    if (props.isFloorHolder) parts.push("speaking");
    else parts.push(props.status);
    return parts.join(", ");
  };

  return (
    <li
      role="listitem"
      aria-label={accessibleLabel()}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "var(--space-3)",
        padding: "var(--space-2) 0",
      }}
    >
      <Avatar name={props.displayName} size="sm" />
      <span
        style={{
          flex: "1",
          "font-size": "var(--text-sm)",
          "font-weight": props.isSelf ? "var(--font-semibold)" : "var(--font-normal)",
          color: props.status === "offline"
            ? "var(--color-text-tertiary)"
            : "var(--color-text-primary)",
          "white-space": "nowrap",
          overflow: "hidden",
          "text-overflow": "ellipsis",
        }}
      >
        {props.displayName}
        {props.isSelf && " (you)"}
      </span>
      {props.isFloorHolder
        ? <span aria-label="Speaking" style={{ display: "flex", "align-items": "center", animation: "pulse 0.67s ease-in-out infinite", color: "var(--color-presence-speaking)" }}><svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="2" width="6" height="11" rx="3"/><path d="M5 10a7 7 0 0 0 14 0"/><line x1="12" y1="19" x2="12" y2="22"/></svg></span>
        : <PresenceDot status={props.status} />
      }
    </li>
  );
};

export default MemberRow;
