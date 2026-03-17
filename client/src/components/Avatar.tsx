import { type Component, createMemo } from "solid-js";

export interface AvatarProps {
  name: string;
  size?: "sm" | "md" | "lg";
}

const SIZES: Record<string, { box: string; font: string }> = {
  sm: { box: "32px", font: "var(--text-xs)" },
  md: { box: "40px", font: "var(--text-sm)" },
  lg: { box: "72px", font: "var(--text-2xl)" },
};

/** Derive a deterministic hue from a name string (0–360). */
function nameToHue(name: string): number {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash) % 360;
}

function getInitials(name: string): string {
  const parts = name.trim().split(/\s+/);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].charAt(0).toUpperCase();
  return (parts[0].charAt(0) + parts[parts.length - 1].charAt(0)).toUpperCase();
}

const Avatar: Component<AvatarProps> = (props) => {
  const sz = () => SIZES[props.size ?? "md"];
  const hue = createMemo(() => nameToHue(props.name));
  const initials = createMemo(() => getInitials(props.name));

  return (
    <div
      role="img"
      aria-label={props.name}
      style={{
        width: sz().box,
        height: sz().box,
        "min-width": sz().box,
        "border-radius": "var(--radius-full)",
        background: `hsl(${hue()}, 50%, 35%)`,
        color: "#fff",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        "font-size": sz().font,
        "font-weight": "var(--font-semibold)",
        "user-select": "none",
        "-webkit-user-select": "none",
      }}
    >
      {initials()}
    </div>
  );
};

export default Avatar;
