// End-to-end test of the web client against `vstimd --null`.
//
// Uses only the public API (src/index.ts) with domain types — no generated
// proto classes, mirroring the Python client's test style. The null renderer
// runs the ZMQ + web servers headlessly, so this exercises the entire web
// command/snapshot path without a GPU or display.
//
//   npm run test:e2e

import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { connect as netConnect } from "node:net";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { Connection, NotSupportedError, rgb } from "../src/index.js";

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const WEB_PORT = 8137; // dedicated test ports; never collide with a real server
const ZMQ_PORT = 5567;
const BASE_URL = `ws://127.0.0.1:${WEB_PORT}`;

function serverBinary(): string {
  // CI sets VSTIMD_BIN to the downloaded artifact (no Rust toolchain needed).
  if (process.env.VSTIMD_BIN) return process.env.VSTIMD_BIN;
  for (const profile of ["release", "debug"]) {
    const bin = join(REPO_ROOT, "target", profile, "vstimd");
    if (existsSync(bin)) return bin;
  }
  const r = spawnSync("cargo", ["build", "--release"], { cwd: REPO_ROOT, stdio: "inherit" });
  if (r.status !== 0) throw new Error(`cargo build --release failed (${r.status})`);
  return join(REPO_ROOT, "target", "release", "vstimd");
}

function portOpen(port: number, timeoutMs = 400): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = netConnect({ host: "127.0.0.1", port });
    const done = (ok: boolean) => {
      sock.destroy();
      resolve(ok);
    };
    sock.setTimeout(timeoutMs);
    sock.once("connect", () => done(true));
    sock.once("timeout", () => done(false));
    sock.once("error", () => done(false));
  });
}

async function waitForPort(port: number, attempts = 40): Promise<void> {
  for (let i = 0; i < attempts; i++) {
    if (await portOpen(port)) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`web server did not open port ${port} in time`);
}

let server: ChildProcess;
let conn: Connection;
let configDir: string;

describe("vstimd web client e2e (--null)", () => {
  beforeAll(async () => {
    // Isolate the VTL shm (name is global) so the test server never collides
    // with a real vstimd the developer may be running.
    const rig = join(tmpdir(), `vstimd-e2e-rig-${process.pid}.toml`);
    writeFileSync(rig, `[vtl]\nshm_name = "/vstimd_e2e_${process.pid}"\n`);
    // Isolated config dir so saved configs don't litter the repo (the default
    // dir is the cwd) and don't collide between runs.
    configDir = mkdtempSync(join(tmpdir(), "vstimd-e2e-cfg-"));
    server = spawn(
      serverBinary(),
      [
        "--null", "--web-port", String(WEB_PORT),
        "--zmq-port", String(ZMQ_PORT), "--rig-config", rig,
        "--config-dir", configDir,
      ],
      { stdio: "ignore" },
    );
    await waitForPort(WEB_PORT);
    conn = await Connection.connect(BASE_URL);
  }, 180_000);

  afterAll(() => {
    conn?.close();
    server?.kill("SIGTERM");
    if (configDir) rmSync(configDir, { recursive: true, force: true });
  });

  it("creates a stimulus and returns a handle", async () => {
    const handle = await conn.stimuli.shapes.createRect({
      pos: { x: 123, y: 45 },
      width: 80,
      height: 40,
      color: rgb(1, 0, 0),
      name: "e2e-rect",
    });
    expect(handle).toBeGreaterThan(0);
  });

  it("reflects created stimuli in the live snapshot", async () => {
    await conn.stimuli.shapes.createRect({
      pos: { x: -200, y: 75 },
      width: 60,
      height: 60,
      color: rgb(0, 1, 0),
      name: "snap-rect",
    });

    const snap = await conn.nextSnapshot();
    // screen_size is seeded under --null, so the map has an aspect ratio.
    expect(snap.serverInfo?.width ?? 0).toBeGreaterThan(0);

    const ours = snap.stimuli.find((s) => s.name === "snap-rect");
    expect(ours, "created stimulus should appear in the snapshot").toBeDefined();
    expect(ours!.kind).toBe("rect");
    expect(ours!.pos.x).toBeCloseTo(-200, 3);
    expect(ours!.pos.y).toBeCloseTo(75, 3);
  });

  it("applies size and orientation setters", async () => {
    const handle = await conn.stimuli.shapes.createRect({ width: 100, height: 50, name: "sized" });
    await conn.stimuli.setRectSize(handle, 240, 120);
    await conn.stimuli.setOrientation(handle, 30);

    const snap = await conn.nextSnapshot();
    const ours = snap.stimuli.find((s) => s.name === "sized")!;
    expect(ours.size.width).toBeCloseTo(240, 3);
    expect(ours.size.height).toBeCloseTo(120, 3);
    expect(ours.orientation).toBeCloseTo(30, 3);
  });

  it("creates a named ellipse with the expected size", async () => {
    await conn.stimuli.shapes.createEllipse({ width: 160, height: 80, name: "ell" });
    const snap = await conn.nextSnapshot();
    const ell = snap.stimuli.find((s) => s.name === "ell")!;
    expect(ell.kind).toBe("ellipse");
    expect(ell.size.width).toBeCloseTo(160, 3);
    expect(ell.size.height).toBeCloseTo(80, 3);
  });

  it("creates a grating", async () => {
    await conn.stimuli.grating.create({ width: 300, height: 200, sf: 0.04, name: "grat", waveform: "sqr" });
    const snap = await conn.nextSnapshot();
    const g = snap.stimuli.find((s) => s.name === "grat")!;
    expect(g.kind).toBe("grating");
    expect(g.size.width).toBeCloseTo(300, 3);
    expect(g.size.height).toBeCloseTo(200, 3);
  });

  it("creates and updates a text stimulus", async () => {
    const h = await conn.stimuli.text.create({ text: "hello", letterHeight: 40, name: "txt" });
    await conn.stimuli.text.setText(h, "world!");
    const snap = await conn.nextSnapshot();
    const t = snap.stimuli.find((s) => s.name === "txt")!;
    expect(t.kind).toBe("text");
  });

  it("names and fires a VTL input line", async () => {
    await conn.vtl.setName(0, 1, "input", "trig");
    await conn.vtl.setInput("trig", true);

    const snap = await conn.nextSnapshot();
    const line = snap.vtlLines.find((l) => l.name === "trig")!;
    expect(line.direction).toBe("input");
    expect(line.bank).toBe(0);
    expect(line.bit).toBe(1);
    expect(line.high).toBe(true);
  });

  it("round-trips a position update (RF-mapping style)", async () => {
    const handle = await conn.stimuli.shapes.createCircle({ radius: 20, name: "drag-me" });
    await conn.stimuli.setPosition(handle, { x: 333, y: -111 });

    const snap = await conn.nextSnapshot();
    const ours = snap.stimuli.find((s) => s.name === "drag-me");
    // The snapshot exposes the handle, so a map UI can drive setPosition from it.
    expect(ours!.handle).toBe(handle);
    expect(ours!.pos.x).toBeCloseTo(333, 3);
    expect(ours!.pos.y).toBeCloseTo(-111, 3);
  });

  it("applies the generic outline / draw-mode setters", async () => {
    const h = await conn.stimuli.shapes.createRect({ width: 40, height: 40, name: "outlined" });
    // No snapshot field for these yet — assert the commands are accepted (no throw).
    await conn.stimuli.setDrawMode(h, "filledAndOutlined");
    await conn.stimuli.setOutlineColor(h, rgb(0, 0, 1));
    await conn.stimuli.setOutlineWidth(h, 3);
    await conn.stimuli.setName(h, "outlined-2");

    const snap = await conn.nextSnapshot();
    expect(snap.stimuli.find((s) => s.name === "outlined-2")).toBeDefined();
  });

  it("sends draw-order commands (server-side gap: throws NotSupported)", async () => {
    // The client builds these correctly, but the server does not yet implement
    // them — see https://github.com/braemons/vstimd/issues/43. Tighten this to
    // assert the reorder once the server supports it.
    const a = await conn.stimuli.shapes.createRect({ width: 10, height: 10, name: "order-a" });
    const b = await conn.stimuli.shapes.createRect({ width: 10, height: 10, name: "order-b" });
    await expect(conn.stimuli.bringToFront(a)).rejects.toBeInstanceOf(NotSupportedError);
    await expect(conn.stimuli.sendToBack(b)).rejects.toBeInstanceOf(NotSupportedError);
    await expect(conn.stimuli.swapDrawOrder(a, b)).rejects.toBeInstanceOf(NotSupportedError);
  });

  it("lists VTL lines via conn.vtl.list()", async () => {
    await conn.vtl.setName(0, 2, "output", "shutter");
    const lines = await conn.vtl.list();
    const ours = lines.find((l) => l.name === "shutter");
    expect(ours).toBeDefined();
    expect(ours!.direction).toBe("output");
    expect(ours!.bank).toBe(0);
    expect(ours!.bit).toBe(2);
  });

  it("creates, arms, and lists an animation", async () => {
    const h = await conn.stimuli.shapes.createRect({ width: 20, height: 20, name: "flash-me" });
    const anim = await conn.animations.flash(h, { durationFrames: 5, name: "flash-anim" });
    expect(anim).toBeGreaterThan(0);

    await conn.animations.arm(anim);
    const list = await conn.animations.list();
    const ours = list.find((a) => a.handle === anim);
    expect(ours).toBeDefined();
    expect(ours!.name).toBe("flash-anim");

    const details = await conn.animations.query(anim);
    expect(details.stimuli).toContain(h);
  });

  it("saves, lists, retrieves, and loads a config", async () => {
    await conn.stimuli.shapes.createRect({ width: 50, height: 50, name: "cfg-rect" });
    await conn.config.save("e2e_web", { overwrite: true });

    expect(await conn.config.list()).toContain("e2e_web");

    const json = await conn.config.retrieve();
    expect(json).toContain("cfg-rect");

    // Reload (replace) and confirm the stimulus is restored.
    await conn.config.load("e2e_web");
    const snap = await conn.nextSnapshot();
    expect(snap.stimuli.find((s) => s.name === "cfg-rect")).toBeDefined();
  });
});
