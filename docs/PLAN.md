# StimServer → Rust Port: Master Plan

> **Status:** Phases 1–4 + 6 complete; Phases 2, 7 partial
> **Last updated:** 2026-03-09
> **Original:** MFC / Direct2D / Direct3D 11 / Windows Named Pipe  
> **Target:** Rust / wgpu / kurbo / ZeroMQ / protobuf / Linux (also Windows-compatible)

### Companion documents

| File | Contents |
|---|---|
| `STIMULUS_DATA_MODEL.md` | Composition-over-inheritance design for the stimulus type system — **read this before implementing Phase 2** |
| `INPUT_LATENCY.md` | Latency analysis for high-rate position input (shared memory vs ZeroMQ) |
| `3D_ROADMAP.md` | Full roadmap for 3-D stimulus support: infrastructure, primitives, corridors/mazes, mesh models, and Gaussian splatting |
| `EVENT_LOGGING.md` | Event logging and replay system: binary log format, messenger thread, ZMQ publication, SQLite export, and deterministic replay |
| `PYTHON_CLIENT.md` | Python client (`wonderlamp_client`): PsychoPy-compatible API, wire protocol, deferred mode, testing strategy, migration guide |
| `BARE_METAL_LINUX.md` | Compositor-free Linux rendering: raw Vulkan (`ash`) + KMS/DRM + libinput — no X11/Wayland required |

---

## Table of Contents

1. [What the Original System Does](#1-what-the-original-system-does)
2. [Target Architecture](#2-target-architecture)
3. [Open Design Questions](#3-open-design-questions)
4. [Crate Dependencies](#4-crate-dependencies)
5. [Protobuf Schema](#5-protobuf-schema)
6. [Module Structure](#6-module-structure)
7. [Phase-by-Phase Implementation Plan](#7-phase-by-phase-implementation-plan)
8. [Key Design Decisions](#8-key-design-decisions)
9. [Migration Path for Existing Clients](#9-migration-path-for-existing-clients)
10. [Suggested Implementation Order](#10-suggested-implementation-order)
11. [Stimulus Data Model Summary](#11-stimulus-data-model-summary)
12. [3-D Stimulus Roadmap Summary](#12-3-d-stimulus-roadmap-summary)
13. [Event Logging and Replay Summary](#13-event-logging-and-replay-summary)

---

## 1. What the Original System Does

### 1.1 Threading Model

The original app is a **two-thread, one-window MFC application**:

| Thread | Role |
|---|---|
| **Display thread** (`CDisplay::PresentLoop`) | Calls `CStimServerDoc::Draw()` then `SwapChain->Present1()` every vsync. Owns the D3D11/D2D render context. |
| **Pipe thread** (`PipeProcedure`) | Blocks on `ReadFile` on a named pipe (`\\.\pipe\StimServerPipe`). Parses 128-byte binary messages, dispatches to `CStimServerDoc::Command()`, writes a 2-byte `short` reply. |

These two threads share state protected by two critical sections (`g_criticalMapSection`,
`g_criticalDrawSection`) and use a **deferred-mode** mechanism: the pipe thread can stage changes
into "copy" fields of each stimulus, then atomically flip them all to live state in one vsync.

### 1.2 Command Protocol (Named Pipe)

Binary messages: `[WORD key (2 bytes), BYTE... body (up to 126 bytes)]`.  
Response: `short` (2 bytes).

- `key == 0` → system commands (clear all, background colour, deferred mode, query screen size /
  frame rate / performance counter, create stimulus/animation objects, etc.)
- `0 < key < 0x8000` → stimulus object commands
- `key >= 0x8000` → animation object commands

On object creation the response is the newly assigned handle. On success `-1`. On error a positive
error code (also latched into a global error mask queryable separately).

### 1.3 Stimulus Types

| C++ Class | Shape | Renderer |
|---|---|---|
| `CStimulusRect` | Axis-aligned rectangle | Direct2D |
| `CEllipse` | Ellipse | Direct2D |
| `CPetal` | Petal (arc + quadratic Bézier) | Direct2D path geometry |
| `CWedge` | Triangle wedge (configurable half-angle) | Direct2D path geometry |
| `CStimulusPic` | Bitmap image | Direct2D bitmap |
| `CStimulusPics` | Multi-frame bitmap sequence | Direct2D bitmap |
| `CStimulusMov` | Video file via Media Foundation | D2D bitmap, decoded frame-by-frame |
| `CStimulusPS` | Custom HLSL pixel shader | D3D11 |
| `CStimulusPart` / `CStimulusParticle` | Point cloud / particles | D3D11 geometry shader |
| `CStimulusSymbol` | Disc / circle stamp used as particle shape | D3D11 + D2D texture |
| `CStimulusPixel` | Single-pixel stimulus | D3D11 viewport |
| `CStimBmpBrush` | Tiled bitmap brush with opacity mask | Direct2D brush |
| `CStimPSpic` | Pixel shader applied to a bitmap | D3D11 |
| `CPhotoDiode` | Photodiode sync flash rectangle in corner | Direct2D |

> **3-D stimuli are not present in the original C++ codebase.** They are a planned addition
> to the Rust port. See `3D_ROADMAP.md` for the full design.

### 1.4 Animation Types

| C++ Class | Motion |
|---|---|
| `CAnimationPath` | Follows a preloaded `(x,y)` path from binary file |
| `CAnimLineSegPath` | Interpolates along line segments at a given speed (px/s) |
| `CAnimHarmonic` | Sinusoidal oscillation along a direction vector |
| `CAnimLinearRange` | Ramps a float parameter from start→end over a duration |
| `CAnimIntegerRange` | Ramps an integer parameter by a fixed increment |
| `CAnimFlash` | Enables stimulus for N frames then finalises |
| `CAnimFlicker` | Toggles stimulus on/off with configurable on/off frame counts |
| `CAnimExternalPositionControl` | Reads `(x,y)` from a **named shared-memory** region every frame |

### 1.5 Deferred Mode

1. Client sends "start deferred mode" → server snapshots all stimulus state into `*_copy` fields.
2. Subsequent commands write into `*_copy` fields only.
3. Client sends "end deferred mode" → server sets `pending_flip = true`.
4. On the very next rendered frame all copy fields are atomically promoted to live fields.

This guarantees that a batch of parameter changes (position, colour, enable/disable of multiple
stimuli) all become visible simultaneously on the same frame — essential for psychophysics.

### 1.6 External Position Control (Shared Memory)

`CAnimExternalPositionControl` opens a named Win32 shared-memory region and reads two `float`
values `(x, y)` every frame in `Advance()`. This is how gaze position, joystick, or other
high-rate inputs control stimulus position with minimal latency — no round-trip to a client
process is needed.

---

## 2. Target Architecture

> The architecture described here covers the **2-D system** (direct port of the C++ app).
> 3-D rendering is additive on top of this — it introduces a second render pass and a camera
> object, but does not change the 2-D pipeline. See `3D_ROADMAP.md` §2 for the evolved
> architecture diagram.

```
wonderlamp_server (Rust, Linux/Windows)
├── zmq_server    (async task / thread)  — protobuf over ZeroMQ REQ/REP
├── input_reader  (thread, optional)     — shared-memory / gamepad / mouse position reader
├── renderer      (main thread)          — wgpu render loop, vsync-locked
├── messenger     (thread)               — event log file, ZMQ PUB, SQLite export
├── overlay       (optional, same thread)— egui or imgui-rs debug overlay
└── SceneState    (Arc<RwLock<…>>)       — shared between all threads
```

### Thread / Task ownership

| Component | Thread | Notes |
|---|---|---|
| winit event loop | Main thread | Required by most platforms |
| wgpu render loop | Main thread (inside event loop) | |
| egui/imgui overlay | Main thread (same render pass) | |
| ZeroMQ REP server | Background thread (tokio) | Protobuf encode/decode |
| ZeroMQ PUB publisher | Messenger thread | Event stream, separate port from REP |
| Shared-memory reader | Background thread (plain `std::thread`) | High-frequency, no allocation |
| Gamepad / mouse input | Background thread or winit events | See §3 |
| Messenger / event log | Background thread (plain `std::thread`) | Owns log file, ZMQ PUB, SQLite; see `EVENT_LOGGING.md` |

---

## 3. Open Design Questions

### 3.1 High-Rate Position Input: Shared Memory vs ZeroMQ

The existing `CAnimExternalPositionControl` reads `(x, y)` from shared memory **every frame**
at display rate (typically 60–240 Hz). The question is whether to keep shared memory or route
this through ZeroMQ.

#### Option A — Keep shared memory (recommended for now)

**Pros:**
- Zero latency: reader thread polls the region every frame, no IPC round-trip.
- Zero copy: just two `f32` reads.
- Compatible with existing producers (eye trackers, gaze servers, custom DAQ code that already
  write to shared memory on Windows/Linux via `mmap`/`shm_open`).
- No serialisation overhead.

**Cons:**
- Platform-specific setup (`shm_open` on Linux, `CreateFileMapping` on Windows — but easily
  abstracted with the `shared_memory` crate).
- Producer must be on the same host (no network).

**Implementation:** Port `CAnimExternalPositionControl` to a Rust struct that holds a
`*const [f32; 2]` mapped view. Expose as `AnimExternalPos { shm_name: String, offset: Vec2 }`.

#### Option B — ZeroMQ SUB/PUB for position streaming

**Pros:**
- Network-transparent: eye tracker on a different machine works.
- Consistent with the rest of the control interface.

**Cons:**
- At 240 Hz display rate and ~100 µs ZMQ round-trip, you get ~2–5 frames of latency.
- Requires the position producer to speak ZMQ.
- Higher CPU overhead (serialise, send, receive, deserialise every frame).

**Verdict:** ZeroMQ is **not fast enough** to replace shared memory for per-frame position
updates at high refresh rates. The latency and jitter would be visible in time-sensitive
experiments. Use a **dedicated ZMQ SUB socket on a separate port** only if the producer is
remote *and* the experiment can tolerate 1–2 frame latency; otherwise keep shared memory.

#### Option C — Hybrid (recommended long-term)

- Keep `AnimExternalPos` using shared memory for low-latency local producers.
- Add a separate `AnimZmqPos` animation type that subscribes to a ZMQ PUB socket for remote /
  networked position streams.
- Let the experiment control which one to use per stimulus.

### 3.2 Gamepad / Mouse / Eye Tracker as Direct Input

Should the application handle input devices directly, or should a separate process own the
device and publish position data?

**Recommendation: separate producer process.**

Reasons:
- Device drivers (eye trackers especially) often ship their own SDK that is painful to embed.
- Keeps the render server simple and dependency-light.
- Any producer (Python script, C++ DAQ program, MATLAB) can write to shared memory or a ZMQ
  PUB socket without modification to the render server.
- Mouse and gamepad *within* the render window can be forwarded via winit events if needed for
  debugging / demo purposes.

**What wonderlamp_server should do:**
- Optionally read mouse position from winit `CursorMoved` events and expose it as a built-in
  `AnimMousePos` animation (useful for testing without hardware).
- Optionally read a gamepad via `gilrs` and expose as `AnimGamepadPos`.
- For production use, `AnimExternalPos` (shared memory) or `AnimZmqPos` (ZMQ SUB) are the
  right tools.

### 3.3 Debug Overlay (egui vs imgui-rs)

An optional immediate-mode overlay showing:
- Current stimulus list (handle, type, enabled, position)
- Animation assignments
- Frame time graph (last 256 frames)
- ZMQ message rate
- Dropped-frame counter
- Screen size / refresh rate

**egui** is recommended over imgui-rs because:
- Pure Rust, no C++ dependency.
- `egui-wgpu` and `egui-winit` integrate cleanly with the existing wgpu/winit stack.
- Actively maintained with good documentation.

The overlay is toggled by **F1** and does **not** affect frame timing when hidden — egui is not
called at all when the overlay is off. The `overlay` Cargo feature is **on by default**; use
`--no-default-features` to strip it from production builds (zero overhead, no egui dependency).

### 3.4 End-to-End Latency Budget

`INPUT_LATENCY.md` §1–8 covers position-input latency in detail. This section addresses the
broader **command-to-photon** pipeline and the render-loop design choices that affect it.

> See `INPUT_LATENCY.md` §9–14 for fuller implementation notes on all topics below.

#### Full pipeline breakdown

| Stage | Mechanism | Typical latency |
|---|---|---|
| Input generated (hardware) | Eye tracker, joystick, DAQ | hardware-dependent |
| Written to shared memory | Producer process | < 1 µs |
| Read by render thread | `AnimExternalPos::advance()` | < 1 µs |
| Tessellate + `write_buffer` | CPU + PCIe DMA | 0.1–0.5 ms |
| GPU renders | wgpu render pass | 0.5–2 ms |
| `surface.present()` blocks | Wait for next vsync | 0–1 frame |
| Display panel response | LCD/OLED hardware | 1–10 ms |
| **Total (120 Hz, best case)** | | **~9–27 ms** |

The dominant terms are the vsync wait (0–8.3 ms at 120 Hz, unavoidable without tearing)
and the display panel response (outside software control). ZMQ command latency adds another
0–1 frame on top when a command arrives mid-frame (see RwLock contention below).

#### Present mode

Use `PresentMode::Fifo` (vsync, double-buffer) for all production use. `Mailbox`
(triple-buffer) reduces the vsync wait by up to one frame but makes it impossible to
determine precisely which vsync a deferred flip was first visible on — unacceptable for
psychophysics timing. Expose `--present-mode [fifo|mailbox]` as a CLI flag for
benchmarking, defaulting to `fifo`.

#### RwLock contention window

The ZMQ server acquires a write lock on `SceneState` to process each command; the render
thread holds a read lock during animation advance and tessellation. A command that arrives
while the render thread holds the lock is delayed until the next frame's setup, silently
adding one frame of latency. This mirrors the original deferred-mode behaviour (writes go
into `*_copy` fields while display runs).

**Client-facing implication:** commands that arrive within the same rendering frame as a
vsync may not be visible until the following frame. For precise one-frame-accurate delivery,
use deferred mode explicitly — the server will hold all changes until `DeferredMode{start:
false}` and flip them atomically on the next frame boundary.

#### Thread priority

The render thread must not be preempted during tessellation and GPU upload. Set OS thread
priority immediately after the winit event loop starts:

- **Linux:** `libc::pthread_setschedparam` with `SCHED_FIFO`, priority 50.
- **Windows:** `SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)`.

Skipping this is the most common cause of multi-millisecond latency spikes on loaded
machines. The ZMQ server thread and messenger thread should run at normal priority.

#### GPU queue depth

Set `SurfaceConfiguration::desired_maximum_frame_latency = 1` to prevent the driver from
buffering more than one frame ahead. Keep the swap chain at 2 images (double-buffer). Do not
request 3 without profiling first.

#### Flip timestamp reporting (open question)

Clients need to know which display frame a deferred flip became visible on. The original
`CmdQueryTimestamp` returned a Win32 performance-counter value from
`IDXGISwapChain1::GetFrameStatistics`. wgpu does not expose swap-chain frame statistics
directly. Options in increasing precision:

1. **Frame counter + known frame rate** — report `(frame_index, frame_rate_hz)` as the
   timestamp payload. Simple, sufficient for most experiment analysis.
2. **`Instant::now()` after `surface.present()`** — wall-clock time, within ~1 ms of the
   actual vsync. Present returns slightly before the vsync on some backends.
3. **wgpu timestamp queries** (`QuerySet` / `QueryType::Timestamp`) — GPU-side timestamps
   latched at render-pass start/end. Requires `TIMESTAMP_QUERY` device feature.
4. **`VK_EXT_present_timing`** — exact Vulkan presentation timestamps; not yet exposed by
   wgpu, driver support varies.

**Recommended for now:** option 1 (frame counter + rate). Graduate to option 3 when
experiment pipelines require sub-millisecond flip timing.

#### VRR / GSync / FreeSync

Disable variable refresh rate on the stimulus display. VRR makes frame durations
non-deterministic, breaking stimulus timing analysis. Disable in the NVIDIA/AMD control
panel on Windows, or via `xrandr` / DRM connector properties on Linux. Add this to the
deployment checklist in the README.

---

## 4. Crate Dependencies

```toml
[package]
name    = "wonderlamp_server"
version = "0.1.0"
edition = "2024"

[features]
default = ["overlay"]   # overlay is ON by default; use --no-default-features to strip
overlay = ["egui", "egui-wgpu", "egui-winit"]

[dependencies]
# Existing (implemented)
bytemuck  = { version = "1",    features = ["derive"] }
indexmap  = "2"
kurbo     = "0.13"
wgpu      = "27.0.1"
winit     = "0.30"
pollster  = "0.3"

# IPC / networking
zeromq   = "0.4"          # pure-Rust ZMQ (no libzmq dependency)
# alternative: zmq = "0.10"  # bindings to libzmq, more battle-tested

# Protobuf
prost    = "0.13"

# Async runtime (for ZMQ server task)
tokio    = { version = "1", features = ["full"] }

# Image loading (bitmaps)
image    = "0.25"

# Shared memory (cross-platform)
shared_memory = "0.12"

# 3-D math — added in Phase A of 3D_ROADMAP.md; harmless to include early
glam = { version = "0.29", features = ["bytemuck"] }

# Event logging (messenger thread + replay)
crossbeam-channel = "0.5"
uuid              = { version = "1", features = ["v4"] }
rusqlite          = { version = "0.32", optional = true, features = ["bundled"] }

# Gamepad (optional)
gilrs    = { version = "0.10", optional = true }

# Debug overlay (feature-gated) — implemented
egui       = { version = "0.33", optional = true }
egui-wgpu  = { version = "0.33", optional = true }
egui-winit = { version = "0.33", optional = true }

# 3-D mesh loading (Phase D of 3D_ROADMAP.md)
gltf = { version = "1", optional = true }
tobj = { version = "4", optional = true }

# Gaussian splatting .ply loading (Phase E of 3D_ROADMAP.md)
ply-rs = { version = "0.1", optional = true }

# Logging
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# CLI argument parsing
clap = { version = "4", features = ["derive"] }

[build-dependencies]
prost-build = "0.13"

[[bin]]
name = "wonderlamp-convert"
path = "src/bin/convert.rs"
required-features = ["rusqlite"]
```

> **Note on zeromq vs zmq:** `zeromq` is a pure-Rust async ZMQ implementation — no native
> dependency, builds cleanly on Linux and Windows. `zmq` wraps libzmq and is more battle-tested
> but requires the C library. Either works; the plan uses `zeromq` for maximum portability.

---

## 5. Protobuf Schema

File: `proto/wonderlamp.proto`

```protobuf
syntax = "proto3";
package wonderlamp;

// ── Top-level envelope ──────────────────────────────────────────────────────

message Request {
    uint32 handle = 1;       // 0 = system, >0 = object handle
    oneof body {
        SystemCmd   system   = 2;
        StimulusCmd stimulus = 3;
        AnimCmd     anim     = 4;
    }
}

message Response {
    oneof result {
        int32  handle = 1;   // newly allocated handle, or -1 for ack
        bytes  data   = 2;   // binary payload for query responses
        string error  = 3;
    }
}

// ── System commands (handle == 0) ───────────────────────────────────────────

message SystemCmd {
    oneof cmd {
        // Scene management
        CmdClearAll          clear_all          = 1;
        CmdSetBackground     set_background     = 2;
        CmdDeferredMode      deferred_mode      = 3;
        CmdSetAllEnabled     set_all_enabled    = 4;
        CmdSetDefaultColor   set_default_color  = 5;
        CmdSetGamma          set_gamma          = 6;
        CmdPhotoDiode        photodiode         = 7;

        // Queries
        CmdQueryScreenSize   query_screen_size  = 10;
        CmdQueryFrameRate    query_frame_rate   = 11;
        CmdQueryTimestamp    query_timestamp    = 12;
        CmdQueryErrorMask    query_error_mask   = 13;

        // Stimulus creation
        CmdCreateRect        create_rect        = 20;
        CmdCreateEllipse     create_ellipse     = 21;
        CmdCreatePetal       create_petal       = 22;
        CmdCreateWedge       create_wedge       = 23;
        CmdCreateDisc        create_disc        = 24;
        CmdCreatePixel       create_pixel       = 25;
        CmdLoadBitmap        load_bitmap        = 26;
        CmdLoadBitmapSeq     load_bitmap_seq    = 27;
        CmdLoadVideo         load_video         = 28;
        CmdLoadWgslShader    load_wgsl_shader   = 29;
        CmdCreateParticles   create_particles   = 30;

        // Animation creation
        CmdCreateAnimPath         create_anim_path      = 50;
        CmdCreateAnimLineSeg      create_anim_lineseg   = 51;
        CmdCreateAnimHarmonic     create_anim_harmonic  = 52;
        CmdCreateAnimLinRange     create_anim_linrange  = 53;
        CmdCreateAnimIntRange     create_anim_intrange  = 54;
        CmdCreateAnimFlash        create_anim_flash     = 55;
        CmdCreateAnimFlicker      create_anim_flicker   = 56;
        CmdCreateAnimExternalPos  create_anim_ext_pos   = 57;
        CmdCreateAnimZmqPos       create_anim_zmq_pos   = 58;
        CmdCreateAnimMouse        create_anim_mouse     = 59;
        CmdCreateAnimGamepad      create_anim_gamepad   = 60;
    }
}

// ── Stimulus commands (0 < handle < 0x8000) ─────────────────────────────────

message StimulusCmd {
    oneof cmd {
        CmdDelete          delete    = 1;
        CmdSetEnabled      enabled   = 2;
        CmdMoveTo          move_to   = 3;
        CmdGetPos          get_pos   = 4;
        CmdSetColor        color     = 5;
        CmdSetSize         size      = 6;
        CmdSetOrientation  orient    = 7;
        CmdSetParam        param     = 8;
        CmdMoveToFront     to_front  = 9;
        CmdSwapOrder       swap      = 10;
        CmdSetProtected    protect   = 11;
        CmdReplaceWith     replace   = 12;
    }
}

// ── Animation commands (handle >= 0x8000) ────────────────────────────────────

message AnimCmd {
    oneof cmd {
        CmdDelete          delete       = 1;
        CmdAssign          assign       = 2;
        CmdDeassign        deassign     = 3;
        CmdSetFinalAction  final_action = 4;
        CmdSetAnimParam    set_param    = 5;
        CmdSetVertices     set_vertices = 6;   // for LineSeg: update path
    }
}

// ── Shared sub-types ────────────────────────────────────────────────────────

message Color { float r = 1; float g = 2; float b = 3; float a = 4; }
message Vec2  { float x = 1; float y = 2; }

// ── System sub-messages ─────────────────────────────────────────────────────

message CmdClearAll        {}
message CmdSetBackground   { Color color = 1; }
message CmdDeferredMode    { bool start = 1; }
message CmdSetAllEnabled   { bool enabled = 1; bool protected_too = 2; }
message CmdSetDefaultColor { Color fill = 1; Color outline = 2; }
message CmdSetGamma        { float exponent = 1; }
message CmdPhotoDiode      {
    bool   enabled  = 1;
    bool   lit      = 2;
    bool   flicker  = 3;
    uint32 position = 4;   // 0=bottom-left, 1=bottom-right
}
message CmdQueryScreenSize {}
message CmdQueryFrameRate  {}
message CmdQueryTimestamp  {}
message CmdQueryErrorMask  {}

// Stimulus creation
message CmdCreateRect      { Vec2 center = 1; float w = 2; float h = 3; Color fill = 4; Color outline = 5; float stroke_width = 6; }
message CmdCreateEllipse   { Vec2 center = 1; float rx = 2; float ry = 3; Color fill = 4; Color outline = 5; float stroke_width = 6; }
message CmdCreatePetal     { float r = 1; float R = 2; float d = 3; float q = 4; Color fill = 5; Color outline = 6; float stroke_width = 7; }
message CmdCreateWedge     { float gamma_deg = 1; Color fill = 2; }
message CmdCreateDisc      { Vec2 center = 1; float radius = 2; Color fill = 3; }
message CmdCreatePixel     { Vec2 pos = 1; Color color = 2; }
message CmdLoadBitmap      { string path = 1; }
message CmdLoadBitmapSeq   { string path = 1; float fps = 2; }
message CmdLoadVideo       { string path = 1; }
message CmdLoadWgslShader  { string path = 1; repeated float params = 2; Vec2 center = 3; Vec2 size = 4; }
message CmdCreateParticles { uint32 width = 1; uint32 height = 2; string path = 3; }

// Animation creation
message CmdCreateAnimPath        { string path = 1; }
message CmdCreateAnimLineSeg     { float speed_px_per_s = 1; repeated Vec2 vertices = 2; }
message CmdCreateAnimHarmonic    { float amplitude = 1; float freq_hz = 2; float direction_deg = 3; float phase_deg = 4; }
message CmdCreateAnimLinRange    { float start = 1; float end = 2; float duration_s = 3; uint32 mode = 4; }
message CmdCreateAnimIntRange    { uint32 start = 1; uint32 end = 2; int32 increment = 3; }
message CmdCreateAnimFlash       { uint32 n_frames = 1; }
message CmdCreateAnimFlicker     { uint32 on_frames = 1; uint32 off_frames = 2; }
message CmdCreateAnimExternalPos { string shm_name = 1; Vec2 offset = 2; }
message CmdCreateAnimZmqPos      { string sub_addr = 1; Vec2 offset = 2; }
message CmdCreateAnimMouse       { Vec2 scale = 1; Vec2 offset = 2; }
message CmdCreateAnimGamepad     { uint32 gamepad_id = 1; uint32 axis_x = 2; uint32 axis_y = 3; Vec2 scale = 4; }

// Per-stimulus sub-messages
message CmdDelete         {}
message CmdSetEnabled     { bool enabled = 1; }
message CmdMoveTo         { Vec2 pos = 1; }
message CmdGetPos         {}
message CmdSetColor       { Color fill = 1; Color outline = 2; float stroke_width = 3; }
message CmdSetSize        { float w = 1; float h = 2; }
message CmdSetOrientation { float angle_deg = 1; }
message CmdSetParam       { uint32 index = 1; float value = 2; }
message CmdMoveToFront    {}
message CmdSwapOrder      { uint32 other_handle = 1; }
message CmdSetProtected   { bool protect = 1; }
message CmdReplaceWith    { uint32 replace_handle = 1; }

// Per-animation sub-messages
message CmdAssign         { uint32 stimulus_handle = 1; }
message CmdDeassign       {}
message CmdSetFinalAction { uint32 mask = 1; }
// final action bits: 0=disable, 2=togglePD, 3=signalEvent, 4=restart, 5=reverse,
//                    6=initialState, 7=endDeferredMode
message CmdSetAnimParam   { uint32 param_id = 1; float value = 2; }
message CmdSetVertices    { repeated Vec2 vertices = 1; }
```

---

## 6. Module Structure

```
wonderlamp_server/
├── build.rs                       # (planned) prost-build: compile proto/wonderlamp.proto
├── proto/
│   └── wonderlamp.proto           # (planned) protobuf schema (see §5)
└── src/
    ├── main.rs                    # entry point, demo scene setup, winit event loop bootstrap
    ├── app.rs                     # winit ApplicationHandler (App struct + event dispatch)
    ├── timing.rs                  # FrameStats, FrameSummary (no wgpu dependency)
    │
    ├── scene/
    │   ├── mod.rs                 # re-exports only
    │   ├── state.rs               # SceneState, handle registry, deferred-mode logic
    │   ├── deferred.rs            # Deferred<T> generic wrapper
    │   ├── animation.rs           # Animation enum + AnimCommon (no concrete variants yet)
    │   ├── photodiode.rs          # PhotoDiodeState
    │   └── stimulus/
    │       ├── mod.rs             # Stimulus enum, stim_field! macro, dispatch methods
    │       ├── common.rs          # StimulusFlags, Transform2D, DrawMode, ShapeAppearance
    │       └── types.rs           # concrete stimulus structs (Rect, Disc, Ellipse, etc.)
    │
    ├── render/
    │   ├── mod.rs                 # re-exports only (Vertex, RenderState)
    │   ├── state.rs               # RenderState: wgpu device/queue/surface, update(), render()
    │   ├── pipeline.rs            # WGSL shader + create_pipeline() (solid colour only)
    │   ├── gpu_buffers.rs         # StimulusMesh, GpuBuffers
    │   ├── tess.rs                # kurbo tessellation (Rect, Disc, Ellipse)
    │   └── overlay.rs             # OverlayRenderer (feature = "overlay", frame timing HUD)
    │
    │   # ── planned ──
    │   # args.rs                  # clap CLI definition
    │   # ipc/zmq_server.rs        # ZeroMQ REP loop
    │   # ipc/shm_reader.rs        # Shared-memory position reader
    │   # input/mouse.rs           # winit CursorMoved → AnimMousePos
    │   # input/gamepad.rs         # gilrs → AnimGamepadPos (feature-gated)
    │   # logging/                 # Event logging, messenger thread, .wllog writer
    │   # replay/                  # Log replay driver
    │   # bin/convert.rs           # wonderlamp-convert: .wllog → SQLite
    │
    └── proto/                     # (planned) generated by build.rs (gitignored)
        └── wonderlamp.rs
```

---

## 7. Phase-by-Phase Implementation Plan

### Phase 3 — Project Scaffolding ✅

- [x] Updated `Cargo.toml` with ZeroMQ / protobuf / tokio dependencies
- [x] `build.rs` — `prost_build::compile_protos` compiles `proto/wonderlamp.proto`
- [x] `proto/wonderlamp.proto` — `CreateRect`, `SetEnabled`, `Delete`, `Request`/`Response` envelope
- [x] `src/proto.rs` — `include!` of generated types from `OUT_DIR`
- [x] `src/lib.rs` — exposes all modules as a library crate for integration tests
- [ ] `args.rs` with `clap` CLI (still to do)

### Phase 1 — Scene State Core (`src/scene/`) ✅

> **Status:** Complete for the implemented command set. Concrete animation implementations still
> to do (Phase 7+).

> **See `STIMULUS_DATA_MODEL.md` for the full design rationale.** The summary is below.

#### Stimulus representation: enum, not trait objects

Stimuli are **not** modelled as `Box<dyn Stimulus>`. Instead, all stimulus types are variants
of a single `Stimulus` enum, and shared state is held in explicit component structs that every
variant composes. This avoids the pitfalls of mirroring the C++ inheritance hierarchy in Rust.

**Implemented in:**
- Component structs (`StimulusFlags`, `Transform2D`, `DrawMode`, `ShapeAppearance`): `src/scene/stimulus/common.rs`
- `Deferred<T>` wrapper: `src/scene/deferred.rs`
- Concrete stimulus structs (Rect, Ellipse, Petal, Wedge, Disc, Bitmap, BitmapSeq, WgslShader, Particle, Pixel): `src/scene/stimulus/types.rs`
- `Stimulus` enum, `stim_field!` macro, dispatch methods: `src/scene/stimulus/mod.rs`

The compiler enforces exhaustiveness — a missing variant in `make_copy` or `flip` is a
**compile error**, not a silent bug (unlike the C++ "forgot to call `Super::makeCopy()`"
failure mode).

#### Animation representation: enum (same pattern as Stimulus)

Animations use a flat `enum Animation` with one variant per animation type, exactly as stimuli
do. `Box<dyn Animation>` is not used. Each variant is a concrete struct holding both
**parameters** (serialized on save) and **runtime state** (tagged `#[serde(skip)]`, reset on
load). Shared bookkeeping (`stimulus_handle`, `final_action`) lives in `AnimCommon`, which
every variant embeds.

`advance()` is a method on the `Animation` enum that dispatches via `match`. An `anim_field!`
macro (parallel to `stim_field!`) eliminates boilerplate in common-field accessors.

See `STIMULUS_DATA_MODEL.md §13` for the full design, struct layouts, and rationale.

**Implemented in:** `src/scene/animation.rs` (trait definition still present; no concrete
impls yet — to be replaced with enum variants as animation types are added in Phase 7+).

#### `SceneState`

**Implemented in:** `src/scene/state.rs` — stimulus/animation registries, handle allocation,
deferred-mode logic (make_copy/flip on all stimuli + background + photodiode).

Still needed: `SceneState::handle_request` is implemented for `CreateRect`, `SetEnabled`, and
`Delete`. Additional commands (move, colour change, animation assignment, etc.) follow the same
pattern in `src/scene/command.rs`.

### Phase 2 — Renderer (`src/render/`)

> **Status:** Partially complete. Module structure finalised (`state.rs`, `pipeline.rs`,
> `gpu_buffers.rs`, `tess.rs`, `overlay.rs`). Solid-colour pipeline working. Rect, Disc,
> and Ellipse tessellation implemented. `FrameStats`/`FrameSummary` in `timing.rs`.
> Fullscreen borderless + Fifo vsync configured. Missing: textured pipeline (bitmaps),
> shader pipeline (custom WGSL), Petal/Wedge tessellation, coordinate-system push constant
> / uniform (currently using raw NDC conversion in tessellator).

**Implemented in:** `render/state.rs` (RenderState), `render/pipeline.rs` (WGSL shader +
`create_pipeline()`), `render/gpu_buffers.rs`, `render/tess.rs`, `render/overlay.rs`.

**Coordinate system:** pixel-space with origin at screen centre, Y-up. The vertex shader
converts to wgpu NDC. Currently using raw NDC conversion in tessellator; still needed:
push constant or uniform for `screen_half_size`.

**Tessellation** (`render/tess.rs`): Rect → 4 vertices, Disc/Ellipse → kurbo centroid fan.
Still needed: Petal (arc + QuadBez) and Wedge (3 line segments) tessellation.

**Pipelines:** Solid-colour pipeline implemented. Still needed: `textured_pipeline` (bitmaps),
`shader_pipeline` (custom WGSL fragment shader per stimulus).

**Render loop** (`RenderState::render()`): acquire surface → deferred flip if pending →
advance animations → clear to background → draw stimuli in insertion order → photodiode →
egui overlay → present → frame stats. See `render/state.rs` for the full implementation.

**Implemented in:** `render/gpu_buffers.rs` (`GpuBuffers`, `StimulusMesh`). Owned exclusively by
the render thread (no locking), mirrors `SceneState::stimuli` by handle. Meshes rebuilt lazily
when `stimulus.needs_rebuild()`. Draw dispatch is a plain `match` — no vtable, fully inlinable.

### Phase 4 — ZeroMQ Server (`src/ipc.rs`) ✅

Implemented in `src/ipc.rs`. `spawn_zmq_thread` starts a dedicated `std::thread` with its own
single-threaded `tokio` runtime running the async ZMQ REP loop. Bind address must use a concrete
IP — `tcp://0.0.0.0:5555` to listen on all interfaces (the `zeromq` crate resolves the host
as DNS; `tcp://*:5555` fails).

Integration tests in `tests/ipc.rs` exercise the full pipeline — real ZMQ socket, real protobuf
encoding — without a GPU, using `free_port()` to avoid conflicts.

### Phase 6 — Threading & Main (`src/main.rs`) ✅

`SceneState` is wrapped in `Arc<RwLock<SceneState>>` and shared between the render thread
(write lock in `update()`, read lock in `render()`) and the ZMQ server thread (write lock per
command). `Animation` is a plain enum (all concrete structs are `Send + Sync` by default since
they contain no raw pointers or thread-local state), so `Arc<RwLock<SceneState>>: Send + Sync`
is satisfied without any special bounds.

`src/lib.rs` exposes all modules as a library crate so integration tests can call
`SceneState::handle_request` directly.

### Python Client (`client-python/`) ✅

`wonderlamp_client.Connection` — thin ZMQ REQ + protobuf wrapper with `create_rect`,
`set_enabled`, and `delete`. Protobuf stubs generated from `server/proto/wonderlamp.proto` live
in `client-python/wonderlamp_client/_proto/wonderlamp_pb2.py`.

`client-python/examples/flash_rects.py` — creates a red and a blue rectangle and flashes them
alternately. Run with `uv run examples/flash_rects.py [--flashes N] [--hz HZ]`.

### Phase 5 — Shared-Memory Position Reader (`src/ipc/shm_reader.rs`)

```rust
pub struct ShmPositionReader {
    shm:    shared_memory::Shmem,
    offset: (f32, f32),
}

impl ShmPositionReader {
    pub fn read(&self) -> (f32, f32) {
        // Safety: two f32 at offset 0, producer writes atomically enough for f32
        let ptr = self.shm.as_ptr() as *const [f32; 2];
        let raw = unsafe { ptr.read_volatile() };
        (raw[0] + self.offset.0, raw[1] + self.offset.1)
    }
}
```

`AnimExternalPos::advance()` calls `reader.read()` and calls `stimulus.move_to()`.
No separate thread needed — runs in the render thread's animation advance step.

### Phase 6 — Threading & Main (`src/main.rs`)

> **Partially implemented:** `main.rs` + `app.rs` handle the winit event loop and render
> bootstrapping. Thread spawning for ZMQ/gamepad not yet wired (depends on Phases 3–5).

```rust
fn main() -> anyhow::Result<()> {
    let args   = Args::parse();
    let scene  = Arc::new(RwLock::new(SceneState::new()));

    // Spawn ZMQ server on its own OS thread (with tokio runtime inside)
    let scene_zmq = Arc::clone(&scene);
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(
            run_zmq_server(scene_zmq, &args.zmq_addr)
        )
    });

    // Gamepad thread (optional feature)
    #[cfg(feature = "gamepad")]
    {
        let scene_gp = Arc::clone(&scene);
        std::thread::spawn(move || input::gamepad::run_gamepad_thread(scene_gp));
    }

    // Main thread: winit event loop + wgpu
    let event_loop = winit::event_loop::EventLoop::new()?;
    let mut app    = App::new(scene, args);
    event_loop.run_app(&mut app)?;
    Ok(())
}
```

### Phase 7 — Deferred Mode (exact port)

> **Status:** Partially complete. `Deferred<T>` wrapper, `make_copy()`/`flip()` on all
> stimulus types + background + photodiode, and `pending_flip` check in render loop are
> all implemented. Missing: wiring to `SceneState::handle_request` (depends on Phase 4).

In `SceneState::handle_request`:
- `DeferredMode { start: true }`:
  1. `make_copy()` on all stimuli.
  2. Snapshot `background`, `photodiode` state into copy fields.
  3. Set `deferred_mode = true`.
- `DeferredMode { start: false }`:
  1. Set `pending_flip = true`.
  2. Clear `deferred_mode` flag.
- At render-loop start: if `pending_flip`:
  1. `get_copy()` on all stimuli.
  2. Apply background copy, photodiode copy.
  3. Set all `dirty` flags.
  4. Clear `pending_flip`.

### Phase 8 — Custom WGSL Pixel Shader Stimuli

Each `WgslShaderStimulus` owns a `wgpu::RenderPipeline` compiled at load time from a
user-supplied `.wgsl` file and a uniform buffer (`ShaderUniforms`: center, size, 8 float
params, phase). `phase` is incremented each frame by `phase_inc` (mirrors C++ `m_phiInc`).

The `ShaderParams` struct is already defined in `src/scene/stimulus/types.rs`. Remaining work:
runtime pipeline compilation, uniform buffer creation/upload, and a `shader_pipeline` in the
render module.

### Phase 9 — Bitmap & Bitmap-Sequence Stimuli

- **Load**: `image` crate → `DynamicImage::into_rgba8()` → `wgpu::Texture` upload.
- **Draw**: textured quad using `textured_pipeline`. UV coords come from a transform matrix
  allowing rotation/scale (port of `CStimulusPic`'s 4×4 transform).
- **Sequence**: Load all frames at startup into a `Vec<wgpu::Texture>`. Advance frame index
  each N render frames based on `fps / frame_rate` ratio (port of `CStimulusPics`).

### Phase 10 — Photodiode Sync Rectangle

Always drawn last (on top) when `photodiode.enabled`. Small filled `Rect` (default: 40×40 px)
in one corner. State machine per frame:
- `flicker = true` → toggle `lit` every frame.
- Draw white if `lit`, skip (don't draw) if `!lit`.
- Position: 0 = bottom-left, 1 = bottom-right (matching the C++ `SetPosition`).

### Phase 11 — Debug Overlay (feature = "overlay")

> **Status:** Substantially complete. `OverlayRenderer` in `render/overlay.rs`.
> The `overlay` feature is **on by default** (use `--no-default-features` to strip it for
> production). Toggle with **F1**. When hidden, egui is not called — zero overhead.
>
> **Implemented panels:**
> - **Frame Timing** — FPS, jitter (std ms), mean/min/max frame time, drop count, frame index; all colour-coded green/yellow/red.
> - **Stimuli** — table of all active stimuli: handle, type name, enabled checkbox (clicking toggles the stimulus live), X and Y position.
> - **Commands** — scrolling log of the last N ZMQ commands: elapsed time, handle, human-readable summary, response; errors shown in red.
>
> **Still planned:** Animation assignments panel, IPC message-rate counter, config/screen-info panel.

Using `egui-wgpu` + `egui-winit`:
- Integrate into the render pass after all stimuli but before present.
- Toggle with **F1**.
- Panels:
  - **Scene**: table of `[handle, type, enabled, position, anim_handle]` for all stimuli.
  - **Animations**: table of `[handle, type, assigned_to, final_action]`.
  - **Timing**: rolling 256-frame graph of frame duration. Show min/max/mean. Highlight dropped
    frames (> 1.5× expected frame time) in red.
  - **IPC**: ZMQ message counter, last message timestamp, last error.
  - **Config**: display index, refresh rate, screen size, ZMQ address.
- When hidden (`!overlay_visible`), egui is not even called — zero overhead.

### Phase 12 — Video Stimuli (deferred)

Use `ffmpeg-next` (Rust bindings to libffmpeg) to decode video frames:
- Decode on a background thread into a ring buffer of `Arc<Vec<u8>>` (RGBA frames).
- Render thread uploads the current frame to a `wgpu::Texture` via `queue.write_texture`.
- This is the most complex item and is explicitly deferred until all other phases are complete.

### Phase 17 — Serde De/Serialization

Add `#[derive(Serialize, Deserialize)]` to all stimulus data types. This is a prerequisite for
the overlay console's `inspect` command (Phase 18) and enables scene save/load.

#### Dependencies

```toml
serde     = { version = "1", features = ["derive"] }
serde_json = "1"
```

#### What gets derived

| Type | Action | Notes |
|---|---|---|
| `StimulusFlags`, `Transform2D`, `ShapeAppearance` | `#[derive(Serialize, Deserialize)]` | Straightforward — all plain data |
| Concrete stimulus structs (`RectStimulus`, `DiscStimulus`, …) | `#[derive(Serialize, Deserialize)]` | New fields appear in output automatically |
| `Stimulus` enum | `#[derive(Serialize, Deserialize)]` | Works naturally; variant name becomes the type tag |
| `Deferred<T>` | Manual impl or `#[serde(serialize_with = …)]` | Serialize the **live** value only; the copy field is transient state and must not be saved |
| `Animation` enum + concrete structs | `#[derive(Serialize, Deserialize)]`; runtime fields tagged `#[serde(skip)]` | Parameters saved; runtime state (frame counter, phase accumulator, etc.) reset to `Default` on load |
| `AnimCommon` | `#[derive(Serialize, Deserialize)]` | `stimulus_handle`, `final_action` |
| `SceneState` | Serialize `stimuli`, `animations`, `background`, `photodiode` | Skip `deferred_mode` flags, `command_log` (runtime state) |

#### Use cases

1. **`inspect <handle>` console command** — `serde_json::to_string_pretty(&stim)` dumps the
   full stimulus struct to the console output. Any new field added to the struct and tagged
   `#[derive(Serialize)]` appears automatically — no manual registration.

2. **Scene save** — serialize `SceneState` to a `.json` file. Useful for snapshotting
   a test setup and reloading it without rerunning a client script.
   ```
   # CLI (Phase 18 console or future args.rs)
   save scene.json
   ```

3. **Scene load** — deserialize a saved `.json` back into a fresh `SceneState`. All tessellation
   `dirty` flags are set so the render thread rebuilds GPU buffers on the next frame. Animations
   are not restored (they must be re-created via ZMQ commands or the console).

4. **Test fixtures** — integration tests can construct expected scene state as JSON strings and
   round-trip through `serde_json::from_str` without needing a running GPU or ZMQ server.

#### File format

JSON is the primary format (human-readable, hand-editable). A future phase may add TOML for
static configuration files (e.g. default background colour, display index) where JSON's lack of
comments is inconvenient.

#### `Deferred<T>` serialization detail

`Deferred<T>` holds a `live: T` and an `Option<T>` copy. Only `live` is meaningful outside of
an active deferred-mode transaction. The serialized form is just the inner `T` with no wrapper:

```json
"transform": { "x": 120.0, "y": -40.0, "angle_deg": 0.0 }
```

On deserialization, `Deferred::new(live)` is constructed; the copy is `None` and
`deferred_mode` is `false`.

### Phase 18 — Overlay Console (feature = "overlay")

An interactive console panel in the egui overlay for issuing commands and inspecting state
without recompiling. Targeted at **developer debugging**: quickly exercising new commands,
inspecting newly added stimulus fields, and reproducing bugs without a full client script.

#### Stage 1 — Line parser (no new dependencies)

A single-line `TextEdit` input with a scrollable output area added to the overlay.

**Built-in commands:**

| Command | Effect |
|---|---|
| `list` | Print all stimulus handles with type and position |
| `inspect <handle>` | Pretty-print the full stimulus struct as JSON (requires Phase 17) |
| `create_rect [x=0] [y=0] [w=100] [h=100] [r=1] [g=1] [b=1]` | Create a rectangle, print returned handle |
| `set_enabled <handle> <true\|false>` | Show or hide a stimulus |
| `move <handle> <x> <y>` | Move a stimulus to pixel coordinates |
| `delete <handle>` | Delete a stimulus |
| `clear` | Delete all stimuli (`CmdClearAll`) |
| `save [path]` | Serialize `SceneState` to JSON file (requires Phase 17) |
| `load <path>` | Replace `SceneState` from a JSON file (requires Phase 17) |

**Threading rule:** the parser runs inside the egui `prepare()` callback on the render thread.
It must **never** call `handle_request` directly. Instead, push parsed `Request` values onto a
`VecDeque<Request>` stored in `SceneState` (or a dedicated `Mutex<VecDeque<Request>>`). The
render thread drains this queue at the top of `update()`, in the same write-lock window as ZMQ
commands. This guarantees console commands have identical timing and error-handling to wire
commands.

**Output area:** a `Vec<ConsoleLine>` capped at 500 entries, each carrying:
```rust
struct ConsoleLine {
    kind: LineKind,   // Input | Output | Error
    text: String,
}
```
Rendered in a `ScrollArea` that sticks to the bottom. Errors shown in red, input echoed in
dim text, output in white.

#### Stage 2 — Rhai scripting (add `rhai = "1"`, pure Rust)

Upgrade the `TextEdit` to multi-line (Shift+Enter for newlines, Enter to submit).
Add a `[Run]` button.

Register Stage 1 functions as [Rhai native functions](https://rhai.rs/book/rust/functions.html):

```rust
engine.register_fn("create_rect", move |x: f64, y: f64, w: f64, h: f64| -> i64 { … });
engine.register_fn("set_enabled", move |handle: i64, enabled: bool| { … });
engine.register_fn("inspect",     move |handle: i64| -> String { … });
// etc.
```

Run the script on a **dedicated side thread** (one per submission); it submits commands back
via the same `VecDeque` channel used in Stage 1. This ensures a `for` loop or `sleep` in a
script cannot block the render thread.

Example script:
```js
let h = create_rect(0.0, 0.0, 200.0, 100.0);
for i in 0..6 {
    set_enabled(h, i % 2 == 0);
    // sleep not available by default — use frame-count delay or a channel signal
}
print(inspect(h));
delete(h);
```

Rhai is chosen over Lua or embedded Python because: pure Rust (no native deps, no build
complexity), designed for embedding in Rust applications, sandboxable by default, and fast
startup (< 1 ms). The function names match the Python client API so the mental model is
consistent for anyone who already uses `wonderlamp_client`.

---

## 8. Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| IPC transport | ZeroMQ REP/REQ | Drop-in replacement for named pipe; cross-platform; language-agnostic clients |
| Message format | protobuf (prost) | Structured, versioned, self-documenting; replaces opaque byte arrays |
| GPU API | wgpu (Vulkan on Linux, DX12/Metal elsewhere) | Already started; cross-platform |
| Path/shape math | kurbo | Already in project; rich Bézier API maps well to D2D geometry types |
| Coordinate system | 2-D: pixel coords, centre origin, Y-up. 3-D: Y-up right-handed world space, units cm | 2-D preserves client compatibility; 3-D matches glTF/Blender defaults |
| Threading | `std::thread` + tokio for ZMQ + `Arc<RwLock<SceneState>>` | Minimal, clear ownership; avoids MFC global state |
| Stimulus representation | `enum Stimulus` (not `Box<dyn Stimulus>`) | No vtable, inline storage in `IndexMap`, exhaustive match catches missing cases at compile time |
| Shared stimulus state | Explicit component structs (`StimulusFlags`, `Deferred<Transform2D>`, `Deferred<Appearance>`) | No implicit data inheritance; every field is exactly where you expect it |
| Deferred copy mechanism | `Deferred<T>` generic wrapper with `.make_copy()` / `.flip()` | Mechanical, cannot be accidentally omitted; compiler enforces all variants in flip |
| Animation representation | `enum Animation` (same as `Stimulus`) | No vtable, inline in `IndexMap`, exhaustive match, `#[derive(Serialize, Deserialize)]` works directly — no separate registry type needed |
| High-rate position input | Shared memory (via `shared_memory` crate) | Sub-frame latency; no serialisation; compatible with existing producers |
| Networked position input | ZMQ SUB socket (`AnimZmqPos`) | For remote producers where 1–2 frame latency is acceptable |
| Video decode | `ffmpeg-next` (Phase 12, deferred) | No pure-Rust alternative with comparable format support |
| Debug overlay | egui (`egui-wgpu` + `egui-winit`), feature-gated | Pure Rust; clean wgpu integration; zero cost when hidden |
| Input devices | Separate producer process recommended | Keeps render server simple; device SDKs stay isolated |
| Mouse/gamepad in-process | `gilrs` for gamepad, winit events for mouse, both optional | Useful for testing without hardware |
| No GUI | Borderless fullscreen `winit` window | Only stimulus display surface needed |
| Display selection | CLI flag `--display-index N` | Replaces MFC "Beamer Dialog" hardware selection UI |
| 3-D rendering | Additive second render pass; 2-D always composited on top | 2-D path never regresses; 3-D pass skipped entirely if no 3-D stimuli present |
| 3-D math | `glam` crate | bytemuck-compatible, wgpu-ecosystem standard, no native deps |
| Camera | `Deferred<Camera3D>` in `SceneState`, addressable as `CAMERA_HANDLE` | Participates in animation system; same `AnimExternalPos` / `AnimHarmonic` etc. work for camera |
| Mesh loading | `gltf` crate (glTF 2.0), `tobj` fallback (OBJ) — both feature-gated | glTF is the open standard; Y-up matches world space convention |
| Gaussian splatting | Long-horizon research target; wgpu compute shader pipeline | Architecture kept open: compute passes, large buffers, and camera design all compatible |
| Event log format | Length-prefixed protobuf records in `.wllog` binary file | Self-describing, appendable, crash-safe, readable from Python/MATLAB via protobuf library |
| Primary log store | Flat binary `.wllog` (always); SQLite optional and deferred | SQLite write latency is non-deterministic; binary append is safe at 240 Hz |
| Log verbosity | Five levels (Error/Warn/Info/Debug/Trace); per-destination config | Trace level enables full replay; Info level is the production default |
| Replay | `--replay file.wllog` replaces ZMQ server with log reader; same `SceneState::handle_request` path | Pixel-identical frame sequence; `ShmSample` events inject logged positions into animations |
| Messenger thread | Dedicated `std::thread`; receives events over `crossbeam_channel::bounded` | Render thread never blocks; all I/O (file, ZMQ PUB, SQLite) isolated to messenger |
| ZMQ PUB events | Separate socket on port 5556; topic-filtered by log level | Independent of REP control channel; subscribers get live event stream without polling |
| `wonderlamp-convert` | Standalone binary in workspace; `.wllog` → SQLite | Post-hoc analysis in Python/R/SQL without touching the server |
| Serialization format | `serde` + JSON (human-readable, hand-editable) | Drives `inspect` console command, scene save/load, and query responses; new fields appear automatically with `#[derive(Serialize)]` |
| Overlay console | Stage 1: line parser (no deps); Stage 2: Rhai scripting (`rhai = "1"`, pure Rust) | Developer debugging use case; commands queued via `VecDeque` and drained in `update()` — same timing as ZMQ commands, render thread never blocked |
| Console scripting language | Rhai (not Lua/Python) | Pure Rust, no native deps, designed for embedding, sandboxable; function names match Python client API |

---

## 9. Migration Path for Existing Clients

Existing clients speak raw binary over a named pipe (Windows only, 128-byte messages). They need
updating to speak protobuf over ZeroMQ. Recommended approach:

### Short Term — Compatibility Shim

Provide a thin **Python adapter library** (`wonderlamp_compat.py`):
```python
# Old API (binary pipe) → new API (protobuf/ZMQ) translation layer
# Existing experiment scripts work unchanged during transition
```

This shim translates the existing binary protocol to the protobuf schema and sends it over ZMQ.
Experiment scripts that import it see the same function signatures as before.

### Long Term — Native Client Libraries

Rewrite client libraries to use protobuf directly:
- Python: `grpcio-tools` or hand-written with `protobuf` + `pyzmq`
- MATLAB: `zmq` MEX + `protobuf` MATLAB library
- C++: `libzmq` + `protobuf`

The structured protobuf messages are far easier to work with than the hand-packed binary format.

---

## 10. Suggested Implementation Order

- [x] **Phase 1** — Scene state: `Stimulus` enum + component structs, `Deferred<T>`, `StimulusFlags`, `Transform2D`, `ShapeAppearance`, `SceneState` with deferred-mode logic, `PhotoDiodeState`, `Animation` trait (no concrete impls yet)
- [x] **Phase 2** *(partial)* — Renderer: `render/` module split (state, pipeline, gpu_buffers, tess, overlay), solid-colour pipeline, Rect/Disc/Ellipse tessellation, frame timing (`FrameStats`/`FrameSummary`), fullscreen borderless + Fifo vsync. Remaining: textured pipeline, shader pipeline, Petal/Wedge tessellation, coordinate-system uniform
- [x] **Phase 3** *(partial)* — Scaffolding: ZeroMQ / protobuf deps, `build.rs`, `proto/wonderlamp.proto`, `src/proto.rs`, `src/lib.rs`. Remaining: `args.rs` CLI
- [x] **Phase 4** — ZeroMQ server: `src/ipc.rs`, wired to `SceneState::handle_request`; integration tests in `tests/ipc.rs`
- [ ] **Phase 5** — Shared-memory reader: `ipc/shm_reader.rs`, `AnimExternalPos`
- [x] **Phase 6** — Main + threading: `Arc<RwLock<SceneState>>` shared between render and ZMQ threads; `Animation: Send + Sync`; `src/lib.rs`
- [x] **Phase 7** *(partial)* — Deferred mode: `Deferred<T>`, `make_copy`/`flip` on all stimuli + background + photodiode, `pending_flip` checked in render loop. Remaining: wire to ZMQ command dispatch
- [x] **Python client** — `wonderlamp_client.Connection` with `create_rect`/`set_enabled`/`delete`; `examples/flash_rects.py`
- [ ] **Phase 8** — WGSL pixel shader stimuli: runtime pipeline compilation, uniform buffer
- [ ] **Phase 9** — Bitmap / bitmap-sequence stimuli: `image` crate, textured pipeline
- [ ] **Phase 10** — Photodiode sync rectangle
- [x] **Phase 11** *(substantially complete)* — egui debug overlay: `OverlayRenderer` with frame timing HUD, stimulus list (with live enable toggles), and command log. `overlay` feature on by default; F1 toggle; zero cost when hidden. Remaining: animations panel, IPC counter, config panel
- [ ] **Phase 12** — Video stimuli via `ffmpeg-next` (deferred, last)
- [ ] **Phase 17** — Serde de/serialization: `#[derive(Serialize, Deserialize)]` on all stimulus structs, `Deferred<T>` live-value serialization, `SceneState` save/load to JSON, test fixtures
- [ ] **Phase 18** — Overlay console: Stage 1 line parser (`list`, `inspect`, `create_rect`, `set_enabled`, `move`, `delete`, `save`, `load`); Stage 2 Rhai scripting; commands queued via `VecDeque`, drained in `update()`
- [ ] **Phase 13** — Event logging: `logging/` module, messenger thread, `.wllog` file writer, ZMQ PUB publisher, `emit!` / `emit_trace!` macros; integrate `EventSender` into `SceneState` and render thread
- [ ] **Phase 14** — Replay mode: `replay/` module, `ReplayDriver`, `LogFileReader`, `--replay` CLI flag, `inject_shm_sample` on position animations, timing modes (real-time, step, as-fast-as-possible)
- [ ] **Phase 15** — SQLite export: `sqlite_writer.rs`, deferred real-time batch writes, `wonderlamp-convert` binary
- [ ] **Phase 16** — Log control protocol: `CmdStartLog`, `CmdStopLog`, `CmdFlushLog`; runtime verbosity change via ZMQ

**3-D roadmap** (see `3D_ROADMAP.md` for full details — each phase is independently shippable):

- [ ] **Phase 3D-A** — 3-D infrastructure: `Camera3D`, depth texture, split render pass (3-D then 2-D), `glam` math, camera uniform buffer, `GpuBuffers3D`
- [ ] **Phase 3D-B** — 3-D primitives: `Box3D`, `Sphere3D`, `Cylinder3D`, `Plane3D` stimulus variants; `Transform3D`, `Material3D` components; unlit + Phong pipelines
- [ ] **Phase 3D-C** — Corridor and maze stimuli: `CorridorStimulus`, `MazeStimulus`; UV-tiled wall textures; camera animation for linear navigation
- [ ] **Phase 3D-D** — Mesh model stimuli: glTF 2.0 loading via `gltf` crate; OBJ fallback via `tobj`; background loading thread; optional skeletal animation
- [ ] **Phase 3D-E** — Gaussian splatting (long horizon, research): `.ply` loading, GPU radix sort compute shader, tile-based splat rasterisation pass

---

## Appendix A — C++ Command Opcode Reference

For reference during port, the original binary command opcodes (key=0 system commands):

| `message[0]` | Description |
|---|---|
| `0` | System: delete all / enable photodiode / enable all stimuli / set background |
| `1` | System: deferred mode / query perf counter / default final action / error mask / default colours / gamma / screen size |
| `2/3` | Load / replace bitmap (`CStimulusPic`) |
| `4/5` | Load / replace pixel shader (`CStimulusPS`) |
| `6` | Load movie (`CStimulusMov`) |
| `8/9` | Load / replace particle (`CStimulusPart`) |
| `10/11` | Create / replace pixel (`CStimulusPixel`) |
| `12/13` | Create / replace symbol (`CStimulusSymbol`) |
| `14/15` | Load / replace bitmap brush (`CStimBmpBrush`) |
| `16` | Photodiode CSR (lit/flicker/position) |
| `18` | Load pixel shader for picture (`CStimPSpic`) |
| `20` | Create rectangle (`CStimulusRect`) |
| `22` | Create particle system (`CStimulusParticle`) |
| `24/25` | Load / replace motion picture (`CStimulusPics`) |
| `26/27` | Create / replace petal (`CPetal`) |
| `28/29` | Create / replace ellipse (`CEllipse`) |
| `30/31` | Create / replace wedge (`CWedge`) |
| `130` | Load animation path (`CAnimationPath`) |
| `132` | Create line-segment path (`CAnimLineSegPath`) |
| `134` | Create harmonic oscillation (`CAnimHarmonic`) |
| `136` | Create linear range (`CAnimLinearRange`) |
| `138` | Create flash (len=3) or flicker (len=5) (`CAnimFlash`/`CAnimFlicker`) |
| `140` | External position control (`CAnimExternalPositionControl`) |
| `142` | Create integer range (`CAnimIntegerRange`) |

Per-stimulus opcodes (key > 0):

| `message[0]` | Description |
|---|---|
| `0` (len=1) | Delete stimulus |
| `0` (len=2) | Enable / disable |
| `3` (len=2) | Set protected flag |
| `3` (len=9) | Move to `(x, y)` |
| `8` | Query position (writes 2× f32 to pipe) |
| `14` (len=1) | Move to front (re-key to highest) |
| `14` (len=3) | Swap draw order with another stimulus |
| other | Forwarded to `stimulus.Command()` |

---

## 11. Stimulus Data Model Summary

**Implemented.** See `STIMULUS_DATA_MODEL.md` for the full design rationale and
`src/scene/stimulus/` for the implementation. Key points:

- **Component structs** (`StimulusFlags`, `Transform2D`, `ShapeAppearance`) composed into each
  concrete stimulus struct — no data inheritance.
- **`Deferred<T>`** wrapper replaces the C++ `makeCopy()`/`getCopy()` virtual chain.
- **`enum Stimulus`** with exhaustive `match` — compiler catches missing variants.
- **`enum Animation`** (same pattern as `Stimulus`) — `AnimCommon` for shared fields, concrete
  structs per animation type, `#[serde(skip)]` on runtime-only fields.

---

## 12. 3-D Stimulus Roadmap Summary

The 2-D system (Phases 1–12 above) is a complete, production-ready visual stimulus server.
3-D stimulus support is entirely additive and does not touch the 2-D pipeline.

**Core principle:** 2-D must never regress. The 3-D render pass runs first (clearing colour
and depth), and the 2-D pass composites on top with no depth test. If no 3-D stimuli are
present, the 3-D pass is skipped entirely — zero overhead for pure 2-D experiments.

**Phased additions to the `Stimulus` enum:**

| Phase | New variants | Key new components |
|---|---|---|
| 3D-A | *(none visible)* | `Camera3D`, depth texture, dual render pass, `glam` |
| 3D-B | `Box3D`, `Sphere3D`, `Cylinder3D`, `Plane3D` | `Transform3D`, `Material3D`, `Vertex3D` |
| 3D-C | `Corridor`, `Maze` | Procedural tessellator, UV tiling, `AnimLinearNav` |
| 3D-D | `Mesh` | glTF/OBJ loading, background thread, optional skinning |
| 3D-E | `GaussianSplat` | Compute sort pass, splat rasterisation, `.ply` loader |

**The camera** is a `Deferred<Camera3D>` in `SceneState`, addressable as `CAMERA_HANDLE`.
All existing animation types (`AnimExternalPos`, `AnimHarmonic`, `AnimLineSegPath`, etc.)
can target the camera handle without modification.

**The `Deferred<T>` mechanism and the `Stimulus` enum** extend naturally to 3-D — no
architectural changes are needed, only new variants and new component structs.

See `3D_ROADMAP.md` for coordinate system definitions, full struct layouts, protobuf schema
additions, open questions (depth precision, anti-aliasing, 2-D/3-D Z ordering), and the
Gaussian splatting implementation strategy.

---

## 13. Event Logging and Replay Summary

Every state-changing input that influences the rendered output is recorded as a protobuf
`LogEvent` message written to a binary `.wllog` file. The system has four destinations
(file, ZMQ PUB, SQLite, egui overlay), each with its own independently configurable
verbosity level.

**The messenger thread** is a dedicated `std::thread` that owns all I/O. All other threads
(render, ZMQ REP, shared-memory reader) emit events via `crossbeam_channel::try_send` — a
non-blocking, non-allocating call. The render thread is never blocked by logging.

**Dual timestamps** are recorded on every event:
- `timestamp_ns` — nanoseconds since session start (`std::time::Instant`, monotonic).
- `frame_index` — the render frame counter, incremented atomically at the start of each frame.

**Verbosity levels** (Error / Warn / Info / Debug / Trace):
- `Info` (default): session lifecycle, object creation/deletion, deferred flips, errors.
- `Debug`: every ZMQ command and response — sufficient for most experiment logging.
- `Trace`: per-frame position samples and frame boundaries — **required for full replay**.
  Enable with `--record` (alias for `--log-level-file trace`).

**Replay** (`--replay session.wllog`) replaces the ZMQ server and all hardware inputs
with a log-file reader. The same `SceneState::handle_request` path is used; `CommandReceived`
events carry raw protobuf bytes that are decoded and replayed verbatim. `ShmSample` /
`ZmqPosSample` events carry the logged `(x, y)` values that are injected into position
animations via `inject_shm_sample`, bypassing shared memory entirely. The result is a
pixel-identical frame sequence on screen.

**SQLite export** is done post-hoc by the standalone `wonderlamp-convert` binary. SQLite is not
written in real time during recording (write latency jitter is unacceptable at 240 Hz). An
optional deferred real-time SQLite feed (5-second lag, Info level only) can be enabled for
the egui overlay's "recent events" panel.

**`wonderlamp-convert`** is a workspace binary:
```
wonderlamp-convert session.wllog session.db [--min-level info]
```

See `EVENT_LOGGING.md` for the full schema, file format specification, messenger thread
design, replay driver implementation, SQLite schema, and open questions (log rotation,
compression, replay accuracy for file-loaded assets).

---

*End of plan. See individual phase sections for implementation details.*
