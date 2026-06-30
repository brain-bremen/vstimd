# vstimd web client — plan & status

Browser control surface for vstimd. Talks to the embedded axum server
(`server/src/web/`) over two WebSocket channels: `/ws` (Request→Response, same
dispatch as ZMQ) and `/events` (SceneSnapshot push).

**Design rationale.** The web server runs as a thread inside the vstimd process,
sharing the same `Arc<RwLock<SceneState>>`/`Arc<Mutex<VtlState>>` as the render
and ZMQ threads and reusing `SceneState::handle_request` — so it adds no
per-frame render cost and no duplicated command logic. Two endpoints, one HTTP
port, each carrying exactly one message type (no envelope). `SceneSnapshot` is
transport-agnostic (reuses existing query messages) and could later feed a ZMQ
`PUB` socket. The web surface is optional at three levels: Cargo feature `web`
(default-on), rig-config `[web] { enabled, port }`, and CLI `--no-web`/`--web-port`.

## Architecture (quick reference)

```
React UI  ──uses──▶  client library (public API)  ──wraps──▶  protobuf-es (src/_proto, private)
   │                         │
   │ /events SceneSnapshot   │ /ws Request→Response
   ▼                         ▼
        embedded axum web server (shares scene/vtl Arcs, reuses handle_request)
```

- Generated protobuf-es stays **private** under `src/_proto/` (gitignored;
  `npm run gen`). Public API (`src/index.ts`) exposes only hand-written domain
  types + namespaced sub-clients — mirrors the Python client's layering.
- Use the **Makefile**, not raw npm: `make dev` (windowed server + UI),
  `make dev-null`, `make build`, `make typecheck`, `make test-e2e`, `make test-ui`.

## Done

- **Client library** (`src/`): `Connection` + `conn.stimuli`/`conn.system`,
  `transport` (CommandTransport/EventTransport), domain types (`types.ts`),
  typed errors (`errors.ts`), snapshot domain mapping (`snapshot.ts`).
- **React UI** (`src/app/`): `useScene` read model, `StimulusMap`
  (true-to-scale canvas, drag-to-move RF mapping), `StimuliPanel`.
- **Tests**: node WS e2e (`tests/e2e.test.ts`, `make test-e2e`) + Playwright
  browser smoke (`playwright/`, `make test-ui`); both spawn isolated `--null`
  backends. Wired into CI (`.github/workflows/ci.yml`).
- Server side: snapshot incl. per-stimulus `handle`; `embed-ui` Cargo feature
  bakes the built UI into the binary; `--web-port`/`--zmq-port`/`--no-web` flags;
  rig-config `[web]`.

## TODO (roughly in priority order)

1. **Expand the client API** to full parity with the Python client / proto: DONE.
   - stimuli: `createEllipse`, grating (`conn.stimuli.grating`: create +
     sf/contrast/phase/drift/opacity/waveform/mask/fore+backColor), text
     (`conn.stimuli.text`: create/setText/setColor), shape setters
     (setOrientation, setRectSize/CircleRadius/EllipseSize, setFillColor, setAlpha),
     and the remaining generic setters (setName, setDrawMode, setOutlineColor,
     setOutlineWidth, bringToFront/sendToBack/swapDrawOrder).
   - `conn.vtl`: list/name/set/toggle/clear lines.
   - `conn.animations`: create (all 7 types) / arm / disarm / delete / list / query,
     with `*Frames`|`*Ms` conversion via a cached server frame rate.
   - `conn.config`: list / load / save / retrieve / upload.
   - JSON Schema for config (schemars server-side, file export + `conn.config.schema()`):
     DEFERRED — do this right before the Config UI panel (step 2) so the panel is
     schema-driven. See "Known issues" for the related server gaps.
2. **UI panels** to match the egui overlay: VTL, Animations, System
   (background/photodiode/deferred), Config (save/load), Log
   (snapshot.commandLog + server log). Creation dialogs for all stimulus types.
3. **Map enhancements**: select + resize/rotate handles (→ setRectSize/
   setCircleRadius/setEllipseSize/setOrientation); grating/text richer rendering;
   click-to-select wired to the panels.
4. **Playwright coverage** grows with the panels (VTL fire, animation arm, config
   round-trip); keep `make test-ui` green.
5. **Deployment / systemd**: the web server is embedded in vstimd (no separate
   unit — it comes up/down with the process). For rig deployment:
   - build/ship vstimd with `--features embed-ui` so the UI is served from the
     binary at the web port (no separate static host);
   - expose the web port + bind address (LAN vs loopback) in the unit / rig-config
     `[web]`, and document firewall;
   - ensure clean shutdown on `systemctl stop` (SIGTERM) — see the known issue;
   - reachable once the server signals `READY=1` (sd_notify already wired).

## Known issues

- **Draw-order commands unimplemented server-side**: `bringToFront` / `sendToBack`
  / `swapDrawOrder` are wired in both clients but the server returns `NotSupported`
  (brain-daemons/vstimd#43). The web e2e asserts the `NotSupported` gap for now.
- **Prod config dir**: default is the cwd (good for dev/tests). For systemd, use
  `StateDirectory=vstimd` + `--config-dir ${STATE_DIRECTORY}/configs`
  (`/var/lib/vstimd/configs`) — runtime-mutable state, not `/etc`. (See step 5.)
- **Shutdown segfault**: vstimd core-dumps on Ctrl-C/SIGTERM with the *windowed*
  backend (both web + ZMQ "shutting down" log lines print first, so it's in
  teardown after `backend.run()` returns). Not reproducible/triagable without a
  backtrace; matters for clean systemd `stop`. Needs: does `make dev-null` also
  crash (discriminates Vulkan vs threads), and a gdb `bt`.
