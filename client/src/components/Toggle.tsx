import { type Component, createUniqueId } from "solid-js";

export interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}

const TRACK_W = 36;
const TRACK_H = 20;
const THUMB = 16;
const THUMB_MARGIN = 2;
const TRAVEL = TRACK_W - THUMB - THUMB_MARGIN * 2;

const checkSvg =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24">' +
      '<path fill="rgba(255,255,255,0.9)" d="M21,7L9,19L3.5,13.5L4.91,12.09L9,16.17L19.59,5.59L21,7Z"/>' +
      "</svg>"
  );

const minusSvg =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24">' +
      '<path fill="rgba(255,255,255,0.7)" d="M19,13H5V11H19V13Z"/>' +
      "</svg>"
  );

const Toggle: Component<ToggleProps> = (props) => {
  const id = createUniqueId();

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "space-between",
        "min-height": "48px",
      }}
    >
      <label
        for={id}
        style={{
          "font-size": "var(--text-base)",
          color: "var(--color-text-primary)",
          cursor: "pointer",
          flex: "1",
        }}
      >
        {props.label}
      </label>
      <button
        id={id}
        role="switch"
        aria-checked={props.checked}
        onClick={() => props.onChange(!props.checked)}
        style={{
          position: "relative",
          width: `${TRACK_W}px`,
          height: `${TRACK_H}px`,
          "min-height": `${TRACK_H}px`,
          "min-width": `${TRACK_W}px`,
          "border-radius": `${TRACK_H / 2}px`,
          border: "none",
          padding: "0",
          cursor: "pointer",
          background: props.checked
            ? "var(--color-brand-primary)"
            : "var(--color-bg-tertiary)",
          transition: "background var(--duration-fast) var(--ease-default)",
          display: "flex",
          "align-items": "center",
          "flex-shrink": "0",
          "margin-left": "var(--space-4)",
          overflow: "hidden",
        }}
      >
        {/* Track icons */}
        <img
          src={checkSvg}
          alt=""
          aria-hidden="true"
          style={{
            position: "absolute",
            left: "4px",
            top: "50%",
            transform: "translateY(-50%)",
            width: "12px",
            height: "12px",
            opacity: props.checked ? "1" : "0",
            transition: "opacity var(--duration-fast) var(--ease-default)",
          }}
        />
        <img
          src={minusSvg}
          alt=""
          aria-hidden="true"
          style={{
            position: "absolute",
            right: "4px",
            top: "50%",
            transform: "translateY(-50%)",
            width: "12px",
            height: "12px",
            opacity: props.checked ? "0" : "1",
            transition: "opacity var(--duration-fast) var(--ease-default)",
          }}
        />
        {/* Thumb */}
        <div
          style={{
            position: "absolute",
            top: `${THUMB_MARGIN}px`,
            left: `${THUMB_MARGIN}px`,
            width: `${THUMB}px`,
            height: `${THUMB}px`,
            "border-radius": "50%",
            background: "#fff",
            transform: props.checked ? `translateX(${TRAVEL}px)` : "translateX(0)",
            transition: "transform var(--duration-fast) var(--ease-default)",
          }}
        />
      </button>
    </div>
  );
};

export default Toggle;
