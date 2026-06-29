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
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { Connection, rgb } from "../src/index.js";

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const WEB_PORT = 8137; // dedicated test port; avoids clashing with a real server
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

describe("vstimd web client e2e (--null)", () => {
  beforeAll(async () => {
    server = spawn(serverBinary(), ["--null", "--web-port", String(WEB_PORT)], { stdio: "ignore" });
    await waitForPort(WEB_PORT);
    conn = await Connection.connect(BASE_URL);
  }, 180_000);

  afterAll(() => {
    conn?.close();
    server?.kill("SIGTERM");
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
});
