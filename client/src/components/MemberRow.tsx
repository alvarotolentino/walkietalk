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
        ? <span aria-label="Speaking" style={{ "font-size": "16px", animation: "pulse 0.67s ease-in-out infinite" }}>🎤</span>
        : <PresenceDot status={props.status} />
      }
    </li>
  );
};

export default MemberRow;
