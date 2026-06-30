import { useScene } from "./useScene.js";
import { StimulusMap } from "./StimulusMap.js";
import { StimuliPanel } from "./StimuliPanel.js";
import { VtlPanel } from "./VtlPanel.js";
import { AnimationsPanel } from "./AnimationsPanel.js";

export function App() {
  const { conn, snapshot, connected } = useScene();
  const info = snapshot?.serverInfo;

  return (
    <div style={{ fontFamily: "sans-serif", color: "#ddd", background: "#1a1a1a", minHeight: "100vh", padding: 16 }}>
      <header style={{ display: "flex", alignItems: "baseline", gap: 16, marginBottom: 12 }}>
        <h1 style={{ margin: 0, fontSize: 20 }}>vstimd</h1>
        <span style={{ color: connected ? "#4c8" : "#c44" }}>
          {connected ? "connected" : "connecting…"}
        </span>
        {info && (
          <span style={{ color: "#888" }}>
            {info.width}×{info.height} @ {info.frameRate.toFixed(0)} Hz · v{info.version}
          </span>
        )}
      </header>
      <div style={{ display: "flex", gap: 24, alignItems: "flex-start" }}>
        <StimulusMap conn={conn} snapshot={snapshot} />
        <StimuliPanel conn={conn} snapshot={snapshot} />
        <VtlPanel conn={conn} snapshot={snapshot} />
        <AnimationsPanel conn={conn} />
      </div>
      <p style={{ color: "#666", fontSize: 12, marginTop: 12 }}>
        Drag a stimulus on the map to move it (receptive-field mapping).
      </p>
    </div>
  );
}
