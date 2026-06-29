import { defineConfig } from "@playwright/test";

// Browser e2e for the React UI. Two managed servers, both on dedicated ports so
// they never collide with a real vstimd the developer may be running:
//   1. vstimd --null on isolated ZMQ + web ports (the backend),
//   2. the Vite dev server, proxying /ws + /events to that backend.
// The test then drives the app in a real browser (boot, connect, create, drag).

const VSTIMD_WEB_PORT = 8138;
const VSTIMD_ZMQ_PORT = 5566;
const UI_PORT = 4173;
const REPO_ROOT = new URL("../..", import.meta.url).pathname;

// Locally, `cargo run` builds + runs the server. In CI (where the binary is
// downloaded as an artifact and the Rust toolchain may be absent), set
// VSTIMD_BIN to the prebuilt binary to skip cargo entirely.
const backendArgs = `--null --web-port ${VSTIMD_WEB_PORT} --zmq-port ${VSTIMD_ZMQ_PORT}`;
const backendCommand = process.env.VSTIMD_BIN
  ? `${process.env.VSTIMD_BIN} ${backendArgs}`
  : `cargo run --release --bin vstimd -- ${backendArgs}`;

export default defineConfig({
  testDir: "./playwright",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  use: { baseURL: `http://127.0.0.1:${UI_PORT}` },
  webServer: [
    {
      command: backendCommand,
      cwd: REPO_ROOT,
      url: `http://127.0.0.1:${VSTIMD_WEB_PORT}/`,
      reuseExistingServer: false,
      timeout: 180_000,
    },
    {
      command: `npm run dev -- --port ${UI_PORT} --strictPort --host 127.0.0.1`,
      env: { ...process.env, VSTIMD_WEB: `http://127.0.0.1:${VSTIMD_WEB_PORT}` },
      url: `http://127.0.0.1:${UI_PORT}`,
      reuseExistingServer: false,
      timeout: 60_000,
    },
  ],
});
