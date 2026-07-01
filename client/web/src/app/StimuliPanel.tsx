// Stimuli list + quick create/delete/visibility, reading from the snapshot and
// issuing commands through the connection.

import { useState } from "react";
import type { Connection, SceneSnapshot } from "../index.js";
import { rgb } from "../index.js";
import { GratingDialog } from "./GratingDialog.js";

interface Props {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
}

export function StimuliPanel({ conn, snapshot }: Props) {
  const stimuli = snapshot?.stimuli ?? [];
  const [showGrating, setShowGrating] = useState(false);

  async function addRect() {
    await conn?.stimuli.shapes.createRect({
      pos: { x: 0, y: 0 },
      width: 120,
      height: 80,
      color: rgb(0.9, 0.2, 0.2),
      name: `rect ${stimuli.length + 1}`,
    });
  }

  async function addCircle() {
    await conn?.stimuli.shapes.createCircle({
      pos: { x: 0, y: 0 },
      radius: 50,
      color: rgb(0.2, 0.6, 0.9),
      name: `circle ${stimuli.length + 1}`,
    });
  }

  return (
    <div style={{ minWidth: 280 }}>
      <h3>Stimuli</h3>
      <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
        <button onClick={addRect} disabled={!conn}>+ Rect</button>
        <button onClick={addCircle} disabled={!conn}>+ Circle</button>
        <button onClick={() => setShowGrating(true)} disabled={!conn}>+ Grating…</button>
      </div>
      {showGrating && conn && (
        <GratingDialog
          conn={conn}
          defaultName={`grating ${stimuli.length + 1}`}
          onClose={() => setShowGrating(false)}
        />
      )}
      <table style={{ width: "100%", fontSize: 13, borderCollapse: "collapse" }}>
        <thead>
          <tr style={{ textAlign: "left", color: "#888" }}>
            <th>On</th><th>Name</th><th>Kind</th><th>Pos</th><th></th>
          </tr>
        </thead>
        <tbody>
          {stimuli.map((s) => (
            <tr key={s.handle}>
              <td>
                <input
                  type="checkbox"
                  checked={s.enabled}
                  onChange={(e) => conn?.stimuli.setEnabled(s.handle, e.target.checked)}
                />
              </td>
              <td>{s.name || <em>—</em>}</td>
              <td>{s.kind}</td>
              <td style={{ fontFamily: "monospace", whiteSpace: "pre" }}>
                {String(Math.round(s.pos.x)).padStart(6)},{String(Math.round(s.pos.y)).padStart(6)}
              </td>
              <td><button onClick={() => conn?.stimuli.delete(s.handle)}>✕</button></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
