// Scene-wide controls: background colour, show/hide all, deferred-mode batching,
// and delete-all. Mirrors the egui System overlay (minus the photodiode toggle,
// which has no runtime command yet — see PLAN).

import type { Color, Connection, SceneSnapshot } from "../index.js";

interface Props {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
}

function colorToHex(c: Color | undefined): string {
  const ch = (v: number) => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, "0");
  return c ? `#${ch(c.r)}${ch(c.g)}${ch(c.b)}` : "#000000";
}

function hexToColor(hex: string): Color {
  return {
    r: parseInt(hex.slice(1, 3), 16) / 255,
    g: parseInt(hex.slice(3, 5), 16) / 255,
    b: parseInt(hex.slice(5, 7), 16) / 255,
    a: 1,
  };
}

export function SystemPanel({ conn, snapshot }: Props) {
  const bg = snapshot?.serverInfo?.background;

  return (
    <div style={{ minWidth: 220 }}>
      <h3>System</h3>

      <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, marginBottom: 10 }}>
        Background
        <input
          type="color"
          disabled={!conn}
          value={colorToHex(bg)}
          onChange={(e) => conn?.system.setBackground(hexToColor(e.target.value))}
        />
      </label>

      <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
        <button disabled={!conn} onClick={() => conn?.system.setAllEnabled(true)}>Show all</button>
        <button disabled={!conn} onClick={() => conn?.system.setAllEnabled(false)}>Hide all</button>
      </div>

      <div style={{ fontSize: 13, color: "#888", marginBottom: 4 }}>Deferred batch</div>
      <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
        <button disabled={!conn} onClick={() => conn?.system.setDeferredMode(true)}>Begin</button>
        <button disabled={!conn} onClick={() => conn?.system.setDeferredMode(false)}>Apply</button>
        <button disabled={!conn} onClick={() => conn?.system.setDeferredMode(false, true)}>Cancel</button>
      </div>

      <button
        disabled={!conn}
        style={{ color: "#e88" }}
        onClick={() => {
          if (confirm("Delete all stimuli?")) conn?.system.deleteAll();
        }}
      >
        Delete all
      </button>
    </div>
  );
}
