// Animation list + lifecycle (arm / disarm / delete), mirroring the egui
// animations overlay. Animations are not part of the SceneSnapshot stream, so
// this panel polls conn.animations.list() and refreshes after each action.

import { useCallback, useEffect, useState } from "react";
import type { AnimationInfo, AnimationState, Connection, SceneSnapshot } from "../index.js";
import { CoupleVisibilityDialog } from "./CoupleVisibilityDialog.js";

interface Props {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
}

const STATE_COLOR: Record<AnimationState, string> = {
  idle: "#888",
  armed: "#cc4",
  running: "#4c8",
  done: "#666",
};

export function AnimationsPanel({ conn, snapshot }: Props) {
  const [anims, setAnims] = useState<AnimationInfo[]>([]);
  const [showCouple, setShowCouple] = useState(false);

  const refresh = useCallback(async () => {
    if (!conn) return;
    try {
      setAnims(await conn.animations.list());
    } catch {
      /* transient — next poll retries */
    }
  }, [conn]);

  useEffect(() => {
    if (!conn) return;
    void refresh();
    const id = setInterval(() => void refresh(), 500);
    return () => clearInterval(id);
  }, [conn, refresh]);

  async function act(fn: () => Promise<void>) {
    await fn();
    await refresh();
  }

  return (
    <div style={{ minWidth: 280 }}>
      <h3>Animations</h3>
      <div style={{ marginBottom: 8 }}>
        <button disabled={!conn} onClick={() => setShowCouple(true)}>+ Couple visibility…</button>
      </div>
      {showCouple && conn && (
        <CoupleVisibilityDialog
          conn={conn}
          snapshot={snapshot}
          defaultName={`couple ${anims.length + 1}`}
          onClose={() => {
            setShowCouple(false);
            void refresh();
          }}
        />
      )}
      {anims.length === 0 ? (
        <p style={{ color: "#666", fontSize: 13 }}>No animations.</p>
      ) : (
        <table style={{ width: "100%", fontSize: 13, borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ textAlign: "left", color: "#888" }}>
              <th>Name</th><th>Type</th><th>State</th><th></th>
            </tr>
          </thead>
          <tbody>
            {anims.map((a) => (
              <tr key={a.handle}>
                <td>{a.name || <em>—</em>}</td>
                <td style={{ color: "#888" }}>{a.typeName}</td>
                <td style={{ color: STATE_COLOR[a.state] }}>{a.state}</td>
                <td style={{ whiteSpace: "nowrap" }}>
                  <button
                    disabled={!conn || a.state === "armed" || a.state === "running"}
                    onClick={() => act(() => conn!.animations.arm(a.handle))}
                  >
                    arm
                  </button>{" "}
                  <button
                    disabled={!conn || a.state === "idle"}
                    onClick={() => act(() => conn!.animations.disarm(a.handle))}
                  >
                    disarm
                  </button>{" "}
                  <button
                    disabled={!conn || (a.state !== "armed" && a.state !== "running")}
                    title="Clean teardown (applies configured cancel actions)"
                    onClick={() => act(() => conn!.animations.cancel(a.handle))}
                  >
                    cancel
                  </button>{" "}
                  <button
                    disabled={!conn}
                    onClick={() => act(() => conn!.animations.delete(a.handle))}
                  >
                    ✕
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
