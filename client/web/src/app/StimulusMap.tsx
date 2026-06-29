// Interactive map: a scaled vector view of the screen reconstructed from the
// snapshot (not a frame stream). Drag a stimulus to move it — the core of manual
// receptive-field mapping. Drags update an optimistic local position immediately
// and send setPosition coalesced to one message per animation frame; the next
// snapshot reconciles.

import { useEffect, useRef, useState } from "react";
import type { Connection, SceneSnapshot, StimulusView, Vec2 } from "../index.js";

interface Props {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
}

const FALLBACK = { width: 1920, height: 1080 };

export function StimulusMap({ conn, snapshot }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Optimistic override for the stimulus currently being dragged.
  const dragRef = useRef<{ handle: number; pos: Vec2 } | null>(null);
  const pendingRef = useRef<Vec2 | null>(null);
  const rafRef = useRef<number | null>(null);
  const [, force] = useState(0);

  const screen = snapshot?.serverInfo
    ? { width: snapshot.serverInfo.width || FALLBACK.width, height: snapshot.serverInfo.height || FALLBACK.height }
    : FALLBACK;

  // Canvas <-> stimulus-space transforms (origin centre, +y up).
  function geom(canvas: HTMLCanvasElement) {
    const scale = Math.min(canvas.width / screen.width, canvas.height / screen.height);
    const cx = canvas.width / 2;
    const cy = canvas.height / 2;
    return {
      toCanvas: (p: Vec2) => ({ x: cx + p.x * scale, y: cy - p.y * scale }),
      toStimulus: (x: number, y: number): Vec2 => ({ x: (x - cx) / scale, y: (cy - y) / scale }),
      scale,
    };
  }

  function stimuli(): StimulusView[] {
    const list = snapshot?.stimuli ?? [];
    const drag = dragRef.current;
    if (!drag) return list;
    return list.map((s) => (s.handle === drag.handle ? { ...s, pos: drag.pos } : s));
  }

  // Redraw whenever the snapshot or drag changes.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const { toCanvas, scale } = geom(canvas);

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    // screen border
    const tl = toCanvas({ x: -screen.width / 2, y: screen.height / 2 });
    ctx.strokeStyle = "#444";
    ctx.strokeRect(tl.x, tl.y, screen.width * scale, screen.height * scale);

    for (const s of stimuli()) {
      const c = toCanvas(s.pos);
      const col = s.fillColor;
      const css = col ? `rgba(${col.r * 255},${col.g * 255},${col.b * 255},${s.enabled ? col.a : 0.25})` : "#888";
      ctx.fillStyle = css;
      ctx.strokeStyle = "#fff";
      if (s.kind === "circle" || s.kind === "ellipse") {
        ctx.beginPath();
        ctx.ellipse(c.x, c.y, 12, 12, 0, 0, Math.PI * 2);
        ctx.fill();
      } else {
        ctx.fillRect(c.x - 12, c.y - 12, 24, 24);
      }
      ctx.fillStyle = "#ccc";
      ctx.font = "11px sans-serif";
      ctx.fillText(s.name || s.kind, c.x + 15, c.y + 4);
    }
  });

  function hitTest(sx: number, sy: number): StimulusView | null {
    const canvas = canvasRef.current!;
    const { toCanvas } = geom(canvas);
    // topmost (last drawn) first
    const list = stimuli();
    for (let i = list.length - 1; i >= 0; i--) {
      const c = toCanvas(list[i].pos);
      if (Math.abs(sx - c.x) <= 14 && Math.abs(sy - c.y) <= 14) return list[i];
    }
    return null;
  }

  function flush() {
    rafRef.current = null;
    const drag = dragRef.current;
    const pos = pendingRef.current;
    if (drag && pos && conn) {
      conn.stimuli.setPosition(drag.handle, pos).catch(() => {});
      pendingRef.current = null;
    }
  }

  function onPointerDown(e: React.PointerEvent<HTMLCanvasElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const hit = hitTest(e.clientX - rect.left, e.clientY - rect.top);
    if (!hit) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { handle: hit.handle, pos: hit.pos };
    force((n) => n + 1);
  }

  function onPointerMove(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!dragRef.current) return;
    const canvas = e.currentTarget;
    const rect = canvas.getBoundingClientRect();
    const { toStimulus } = geom(canvas);
    const pos = toStimulus(e.clientX - rect.left, e.clientY - rect.top);
    dragRef.current = { handle: dragRef.current.handle, pos };
    pendingRef.current = pos;
    if (rafRef.current == null) rafRef.current = requestAnimationFrame(flush);
    force((n) => n + 1);
  }

  function onPointerUp(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!dragRef.current) return;
    flush();
    dragRef.current = null;
    e.currentTarget.releasePointerCapture(e.pointerId);
    force((n) => n + 1);
  }

  return (
    <canvas
      ref={canvasRef}
      width={960}
      height={540}
      style={{ background: "#111", border: "1px solid #333", touchAction: "none", cursor: "crosshair" }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    />
  );
}
