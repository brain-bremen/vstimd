// Named scene-config persistence against the server's config directory — the web
// counterpart of the overlay's save/load file browser. Simple list/load/save;
// a schema-driven editor is deferred (see brain-daemons/vstimd#45).

import { useCallback, useEffect, useState } from "react";
import type { Connection } from "../index.js";

interface Props {
  conn: Connection | null;
}

export function ConfigPanel({ conn }: Props) {
  const [names, setNames] = useState<string[]>([]);
  const [saveName, setSaveName] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!conn) return;
    try {
      setNames(await conn.config.list());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [conn]);

  useEffect(() => {
    void refresh();
  }, [conn, refresh]);

  async function run(action: () => Promise<void>) {
    setError(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  function save() {
    const name = saveName.trim();
    if (!name || !conn) return;
    void run(async () => {
      await conn.config.save(name, { overwrite });
      setSaveName("");
    });
  }

  return (
    <div style={{ minWidth: 220 }}>
      <h3>Config</h3>

      <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 4 }}>
        <input
          type="text"
          placeholder="save as…"
          value={saveName}
          disabled={!conn}
          onChange={(e) => setSaveName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
          style={{ flex: 1, minWidth: 0 }}
        />
        <button disabled={!conn || !saveName.trim()} onClick={save}>Save</button>
      </div>
      <label style={{ fontSize: 12, color: "#888", display: "block", marginBottom: 10 }}>
        <input type="checkbox" checked={overwrite} onChange={(e) => setOverwrite(e.target.checked)} />
        {" "}overwrite if exists
      </label>

      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
        <span style={{ fontSize: 12, color: "#888" }}>Saved configs</span>
        <button disabled={!conn} onClick={() => void refresh()} style={{ fontSize: 11 }}>↻</button>
      </div>
      {names.length === 0 ? (
        <p style={{ color: "#666", fontSize: 13 }}>None.</p>
      ) : (
        <table style={{ width: "100%", fontSize: 13, borderCollapse: "collapse" }}>
          <tbody>
            {names.map((name) => (
              <tr key={name}>
                <td>{name}</td>
                <td style={{ whiteSpace: "nowrap", textAlign: "right" }}>
                  <button
                    disabled={!conn}
                    title="Replace the scene with this config"
                    onClick={() => void run(() => conn!.config.load(name))}
                  >
                    Load
                  </button>{" "}
                  <button
                    disabled={!conn}
                    title="Merge this config into the current scene"
                    onClick={() => void run(() => conn!.config.load(name, { additive: true }))}
                  >
                    Load+
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {error && <div style={{ color: "#e88", fontSize: 12, marginTop: 8 }}>{error}</div>}
    </div>
  );
}
