# Web Interface for vstimd

## Context

vstimd currently has two control surfaces: a Python/ZMQ client (REQ/REP, binary
protobuf) and an in-process **egui overlay** (`server/src/render/overlay_ui/`)
that mutates `Arc<RwLock<SceneState>>` directly and is drawn by the render thread
every frame. We want a **browser-based control surface** that:

- covers everything the egui overlay does (stimuli, VTL, animations, system info, config save/load);
- has **minimal/zero impact on render-thread performance** (must not render per frame);
- lets users **configure stimuli, VTL, animations**;
- lets users **position stimuli on a map** (a scaled vector view of the screen);
- supports **manual receptive-field (RF) mapping**: drag a stimulus with the mouse at low latency;
- needs **no authentication**.

**Key architectural insight (validated against the code):** the ZMQ thread already
turns a `proto::Request` into a `proto::Response` by calling
`SceneState::handle_request(req, vtl_ref)` under a brief write lock
(`server/src/ipc.rs:198-204`). A web server thread can clone the *same* `scene`
and `vtl` Arcs (created in `server/src/main.rs:57-78`, already cloned into the ZMQ
thread at `main.rs:108-112`) and reuse that exact dispatch path. This means **no
duplicated command logic, no second source of truth, and no per-frame cost** — the
web thread only touches the lock when a command arrives or when it samples a
snapshot. The egui overlay stays unchanged; the two control surfaces coexist.

**Decisions (confirmed with user):** embed an axum HTTP+WebSocket server in the
process; binary protobuf on the wire (reuse `.proto` via protobuf-es on the
frontend); React + Vite + TypeScript frontend; build the full feature set
(panels + map + RF mapping) before release.

## Architecture

```
Browser (React + Vite + TS)
  │  protobuf-es generated client (from existing proto/)
  │
  ├── WebSocket  /ws
  │     client → server : Request   (binary protobuf, identical to ZMQ payload)
  │     server → client : SceneSnapshot (binary protobuf, new message)
  │
  └── HTTP  GET /        static React bundle (embedded via rust-embed)

vstimd process
  ├── render thread        (unchanged; egui overlay still works)
  ├── zmq-server thread     (unchanged)
  └── web-server thread     NEW — axum on its own current-thread tokio runtime
        • shares Arc<RwLock<SceneState>> + Arc<Mutex<VtlState>>
        • each WS Request  → scene.handle_request(req, vtl) (same as ipc.rs)
        • snapshot pump    → ~30 Hz: read-lock, build SceneSnapshot, broadcast
```

Why this shape:
- **Lowest latency for RF mapping** — no extra process/network hop; a drag becomes
  a `SetPosition` Request handled directly under the write lock between frames.
- **Render thread untouched** — the web thread never participates in the frame loop.
  Its only lock interactions are brief per-command writes and ~30 Hz snapshot reads.
- **Self-contained binary** — embedding the frontend (rust-embed) keeps Jetson Nano
  deployment a single executable, consistent with the systemd deployment model.

## Server-side work (Rust)

### 1. New proto message for server→client push — `proto/vstimd/v1/web.proto`
There is currently **no** snapshot/subscription type (ZMQ is pure REQ/REP). Add one,
reusing existing payload messages so there is zero drift:

```proto
message SceneSnapshot {
  QueryServerInfoResponse        server_info = 1;   // size, frame_rate, background, backend
  repeated QueryStimulusResponse stimuli     = 2;   // full geometry per stimulus (see below)
  ListAnimationsResponse         animations  = 3;
  ListVirtualTriggerLinesResponse vtl_lines  = 4;
  VirtualTriggerLineStateResponse vtl_state  = 5;   // current levels + rise/fall latches
  uint64 frame_count    = 6;
  uint64 server_time_ns = 7;
  // recent IPC command log entries for the Log panel (new small message)
  repeated CommandLogEntry command_log = 8;
}
message CommandLogEntry { uint32 handle = 1; string summary = 2; int32 code = 3; uint64 server_time_ns = 4; }
```

`QueryStimulusResponse` (`proto/vstimd/v1/stimuli/query.proto`) already carries
everything the map needs: `stimulus_type`, `enabled`, `pos`, `orientation`,
`opacity`, `fill_color`, `outline_color`, `outline_width`, `draw_mode`,
`params` (per-shape geometry incl. grating params), `draw_order`, `id`, `name`.
Reuse the existing builder that produces it for `QueryStimulusRequest` (in
`server/src/scene/command.rs`) so the snapshot path and the query path agree.

Add `web.proto` to the Python Makefile glob and `buf`/protobuf-es generation (no
Python code change needed; just keeps stubs complete).

### 2. Web server module — `server/src/web/mod.rs` (new)
Model it on `server/src/ipc.rs`:
- `spawn_web_thread(scene, vtl, log_buffer, bind_addr) -> (JoinHandle, shutdown_tx, bound_rx)`
  — dedicated `std::thread` with a `tokio::runtime::Builder::new_current_thread()`,
  exactly like `spawn_zmq_thread` (`ipc.rs:45-65`).
- axum router:
  - `GET /` and static assets → **rust-embed** of the built React `dist/`.
  - `GET /ws` → WebSocket upgrade.
- **WS receive task**: decode each binary frame with `proto::Request::decode`
  (same as `ipc.rs:157`), then run the same dispatch block as `ipc.rs:198-204`:
  `scene.write()` → `handle_request(req, vtl_ref)` → set `frame_count`/`server_time_ns`.
  Reply with the encoded `Response` on the same socket (so the client can match
  create→handle/UUID). Handle `WaitForFrames`/`WaitUntil` the same way ipc.rs does,
  or simply reject them for the web client in the MVP (the UI doesn't need them).
- **Snapshot pump**: a `tokio::time::interval(~33 ms)` task that takes a brief
  `scene.read()` (+ `vtl.lock()`), builds `SceneSnapshot`, and broadcasts via a
  `tokio::sync::broadcast` channel to all connected WS senders. Skip building if no
  clients are connected. Rate is independent of the render loop.
- Server log lines for the Log panel: clone the existing `log_buffer`
  (`Arc`, created `main.rs:20`, currently passed only to backends) into the web
  thread; include recent lines in the snapshot or a separate WS message.

### 3. Wire it up — `server/src/main.rs` + `server/src/lib.rs`
- Add `pub mod web;` to `lib.rs`.
- After `spawn_zmq_thread` (`main.rs:108`), call `spawn_web_thread(scene.clone(),
  vtl.clone(), log_buffer.clone(), &format!("0.0.0.0:{web_port}"))`.
- New CLI flags in `parse_args`: `--web-port <N>` (default e.g. `8080`) and
  `--no-web` to disable. Mirror the `zmq_bound` ready-wait if desired.
- On shutdown, drop the web shutdown sender and join, mirroring `main.rs:143-144`.
- `log_buffer` is currently moved into the backend (`main.rs:128/135`); change to
  `.clone()` (or wrap the relevant part in an `Arc`) so both the backend and web
  thread can read it. Verify `LogBuffer` is `Clone`/`Arc`-backed first.

### 4. Cargo deps (`server/Cargo.toml`)
Add `axum` (ws + http), `tower-http` (only if not embedding), `rust-embed`,
`tokio-tungstenite` is pulled by axum's `ws`. `tokio` and `prost` already present.

## Frontend work (`client/web/`, new)

Vite + React + TS. Buf/protobuf-es generation from `proto/` produces typed
`Request`/`Response`/`SceneSnapshot` (add a `client/web/buf.gen.yaml` and a
`make proto`-style script). Build output goes to `client/web/dist/`, embedded by
rust-embed at compile time.

A single WS connection (`useWebSocket` hook) holds the latest `SceneSnapshot` in
React state (the **read model**) and exposes a `send(req: Request)` to mutate
(the **command model**). All panels render from the snapshot; all edits send a
`Request`. Optimistic local updates for drag (below) reconciled by the next snapshot.

### Panels (parity with egui overlay `overlay.rs`)
- **Stimuli** — list from `snapshot.stimuli`; create dialog → `CreateRect/Circle/
  Ellipse/Grating/Text`; per-row enable (`SetEnabled`), delete (`Delete`),
  edit position/size/colors/orientation/draw-order. Grating sub-form maps to the
  `SetGrating*` commands.
- **VTL** — banks + per-line level/rise/fall from `vtl_state`; fire buttons →
  `SetInput/Toggle*VirtualTriggerLine`; rename → `SetVirtualTriggerLineName`.
- **Animations** — list from `animations`; create dialog (Flash/Flicker/
  EnableOnEdge/CoupleVisibility/MoveSegments) → `CreateAnimation`; Arm/Disarm/
  Delete; (trigger via the VTL fire commands).
- **System** — `server_info` (size, refresh, backend, version); background color →
  `SetBackground`; photodiode + deferred-mode toggles via their Requests.
- **Config** — `ListConfigs`/`LoadConfig`/`UploadConfig`/`RetrieveConfig`.
- **Log** — `command_log` + server log lines from the snapshot.

### The Map + RF mapping (core interactive view)
- A `<canvas>` (or SVG) sized to the screen aspect from `server_info`, with a
  scale factor mapping **stimulus space (origin = centre, Y-up, pixels)** to canvas
  pixels. The coordinate convention is fixed in the code: position `(x, y)` =
  x px right, y px up from centre (`scene/stimulus/transform2d.rs`,
  `render/benchmark.rs`). Mouse→stimulus: `x = mx - W/2`, `y = H/2 - my`.
- Draw each `snapshot.stimuli` entry as its vector approximation (rect/circle/
  ellipse outline+fill, grating as a tinted box with an orientation tick, text as a
  label box). This is a cheap reconstruction — **not** a stream of rendered frames.
- **Drag to move (RF mapping):** on `pointermove` while dragging, update the local
  optimistic position immediately and, coalesced to one message per
  `requestAnimationFrame`, send `SetPosition`. The web thread applies it under the
  write lock between frames → low latency. The ~30 Hz snapshot reconciles.
- Click-to-select, drag-handles for resize (→ `SetRectSize`/`SetCircleRadius`/
  `SetEllipseSize`), and a rotate handle (→ `SetOrientation`) reuse the same path.

## Critical files
- New: `proto/vstimd/v1/web.proto`, `server/src/web/mod.rs`, `client/web/` (React app).
- Modified: `server/src/lib.rs` (`pub mod web`), `server/src/main.rs` (spawn web
  thread, CLI flags, `log_buffer` clone), `server/Cargo.toml` (axum/rust-embed),
  `client/python/Makefile` (add `web.proto` to glob).
- Reused as-is: `SceneState::handle_request` + `handle_system_command`/
  `handle_stimulus_command` (`server/src/scene/command.rs`), the query-response
  builder for `QueryStimulusResponse`, the response helpers in `ipc.rs:69-113`,
  the spawn/shutdown pattern in `ipc.rs:45-65`.

## Testing strategy

The web server thread has **no GPU/render-thread dependency** — it shares the
`scene`/`vtl` Arcs and reuses `handle_request`. So the **null renderer is enough
to test the entire web client** (commands, snapshot pump, RF-mapping mutations).
The only thing `--null` cannot test is on-screen pixels, but the web map is a
*reconstruction from scene state*, not a frame stream, so it is fully testable.

**Prerequisite fix for null-mode testing:** under `--null`, `runtime.screen_size`
is `None`, so `QueryServerInfoResponse` returns `(0,0)` (`scene/command.rs:897`)
and the map has no aspect ratio. Make the null backend (`render/null_backend.rs`)
set `runtime.screen_size` from rig-config (the rig display preference is already
loaded in `main.rs:39-44` but "not yet applied") or a sensible default
(e.g. 1920×1080), optionally overridable via the existing `--windowed WxH`-style
size arg. Then `server_info` is populated headlessly.

Three layers:

1. **Frontend unit tests (vitest, no server):** coordinate transforms
   (mouse↔stimulus space: `x = mx - W/2`, `y = H/2 - my`), snapshot→view reducers,
   protobuf-es encode/decode round-trips, drag-coalescing logic. Fast, run in CI.

2. **Wire e2e — node WS client vs `vstimd --null` (headless, no browser):** the
   fastest true e2e. A node test harness starts `vstimd --null`, opens a WebSocket
   using the *same* protobuf-es client the app uses, sends `Request`s and asserts
   `Response`s + the pushed `SceneSnapshot`. Cross-check with the existing Python
   ZMQ client that both surfaces observe identical `SceneState` (create via web →
   `list_stimuli` via Python and vice-versa). This validates the whole server web
   path without a browser. Add a `client/web/Makefile` (or npm script) target
   `test-e2e-null` mirroring `client/python`'s `make test-e2e-null`.

3. **Browser e2e (Playwright) vs `vstimd --null`:** start `vstimd --null`, serve
   the app (the embedded bundle at `http://localhost:8080`, or `vite preview`),
   drive real interactions — create stimuli, fire VTL, arm animations, and
   crucially **canvas drag for RF mapping** (`page.mouse` down/move/up) — then
   assert both the DOM and the resulting server state (queried via the WS client or
   Python). This is the "proper testing of the web UI" end-to-end.

CI wiring: a `make test-e2e-null` that builds the server, launches it with
`--null`, waits for the web port to bind (mirror `wait_zmq_bound` in `main.rs`),
runs layers 2–3, and tears down — analogous to the Python client's e2e flow.

## Verification
- **Build:** `cargo build --release` (server compiles with web thread); in
  `client/web/`: `npm install && npm run build` produces `dist/` embedded by the
  server. `cargo clippy`, `cargo test` stay green (handle_request unchanged).
- **Run:** `cargo run --release -- --windowed 1280x720` (or `--null` for headless),
  open `http://localhost:8080`. Confirm the egui overlay still works in parallel.
- **Parity check:** create a rect/grating from the web UI; confirm it appears in
  the egui Stimuli panel and on screen, and vice-versa (egui-created stimulus shows
  in the web map). Fire a VTL line and arm an animation from the web UI; confirm
  state matches the egui VTL/Animation panels.
- **RF mapping latency:** drag a stimulus on the map; confirm it tracks the cursor
  on the rendered display with low latency and no render-thread frame drops
  (watch the System panel / benchmark dropped-frame counter).
- **Cross-client:** run the Python e2e suite (`cd client/python && make test-e2e`)
  unchanged to confirm the ZMQ path and `handle_request` are unaffected.
- **Perf guard:** with the web UI open and idle, verify frame timing in the egui
  System panel is unchanged vs. web UI closed (snapshot pump + idle WS should be
  negligible; the pump skips work when no client is connected).

## Notes / risks
- `proto::Request`/`Response` are prost types; the browser uses protobuf-es from the
  same `.proto`, so binary frames are byte-compatible with the ZMQ payload — no JSON
  conversion layer.
- The snapshot pump uses a **read** lock; the render thread takes a **write** lock
  during tessellation. Brief contention only; if ever an issue, gate the pump behind
  a dirty flag or lower its rate. Drag commands take the **write** lock like any ZMQ
  command already does.
- MVP omits `WaitForFrames`/`WaitUntil` over WS (not needed by an interactive UI);
  add later if a web client needs frame-accurate sequencing.

## Transport refinement (supersedes the single-`/ws` sketch above)

No `ServerEvent` envelope and no multiplexing. **Two WebSocket endpoints on one
HTTP port**, each carrying exactly one message type:
- `/ws` — command channel: client `Request` → server `Response` (pure REQ/REP,
  same dispatch as ZMQ).
- `/events` — state channel: server pushes raw `SceneSnapshot` frames.

Kept on one port via separate endpoints (a second TCP port adds bind/flag/firewall
overhead with no browser benefit). `SceneSnapshot` is transport-agnostic and could
later feed a ZMQ `PUB` socket for non-browser subscribers without changes.

## Implementation status (server side complete)

Done and compiling; verified running under `--null` (HTTP index serves; web + ZMQ
coexist). Not yet committed.
- `proto/vstimd/v1/snapshot.proto`: `SceneSnapshot` + `CommandLogEntry` (transport-agnostic, not web-named); wired into `build.rs` (both lists).
- `scene/command.rs`: extracted `query_stimulus_response()` (shared with `QueryStimulus`)
  and added single-pass `SceneState::build_snapshot(&self, vtl)` (read-lock only, no
  per-stimulus dispatch).
- `server/src/web/mod.rs`: axum server, `spawn_web_thread`, `/ws` + `/events`,
  ~30 Hz broadcast snapshot pump, reuses `handle_request` exactly like `ipc.rs`.
- `lib.rs` (`pub mod web`), `main.rs` (spawn + `--web-port`/`--no-web` + shutdown +
  rig-config `screen_size` seed for null mode), `Cargo.toml` (axum 0.8, futures-util,
  tokio `net`/`macros`). The `screen_size` prerequisite is implemented in `main.rs`
  (seeded from rig-config / default 1920×1080), not in `null_backend.rs`.
- **Web is optional at three levels** (precedence: CLI ?? rig-config ?? default):
  1. Cargo feature `web` (default-on) — `cargo build --no-default-features` compiles
     out `src/web` and drops `axum`/`futures-util`. Gated via `#[cfg(feature="web")]`.
  2. rig-config `[web]` section — `WebRigConfig { enabled: bool = true, port: u16 = 8080 }`.
  3. CLI — `--no-web`, `--web-port <N>` (Option-typed; override rig-config).

Remaining: WS protobuf round-trip e2e harness (node/python), React frontend under
`client/web/` with protobuf-es codegen + rust-embed of `dist/`, and a
`make test-e2e-null` target.
