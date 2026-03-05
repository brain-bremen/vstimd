# StimServer → Rust Port: Master Plan

> **Status:** Planning phase  
> **Last updated:** 2026
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

The overlay should be toggled by a hotkey (e.g. F1) and must **not** affect frame timing when
hidden — it should be compiled out of the render pass entirely when the `overlay` Cargo feature
is not enabled, so production builds have zero overhead.

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
buffering more than one frame ahead. Higher values reduce GPU idle time but add a full
frame of latency per extra buffer:

```rust
config.desired_maximum_frame_latency = 1;
```

Keep the swap chain at 2 images (double-buffer). Do not request 3 without profiling first.

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
default = ["overlay"]
overlay = ["egui", "egui-wgpu", "egui-winit"]

[dependencies]
# Existing
bytemuck = { version = "1",    features = ["derive"] }
kurbo    = "0.13"
wgpu     = "27"
winit    = "0.30"
pollster = "0.3"

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

# Debug overlay (feature-gated)
egui      = { version = "0.29", optional = true }
egui-wgpu = { version = "0.29", optional = true }
egui-winit = { version = "0.29", optional = true }

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
├── build.rs                       # prost-build: compile proto/wonderlamp.proto → OUT_DIR
├── proto/
│   └── wonderlamp.proto
└── src/
    ├── main.rs                    # CLI args, thread spawning, winit event loop
    ├── args.rs                    # clap CLI definition
    │
    ├── scene/
    │   ├── mod.rs                 # SceneState, handle registry, deferred-mode logic
    │   ├── stimulus.rs            # Stimulus trait + all variants
    │   ├── animation.rs           # Animation trait + all variants
    │   └── photodiode.rs          # PhotoDiode sync flash state
    │
    ├── render/
    │   ├── mod.rs                 # RenderState: wgpu Device/Queue/Surface, main render fn
    │   ├── tessellator.rs         # kurbo BezPath → Vertex/Index buffers
    │   ├── gpu_stimulus.rs        # GpuStimulus: per-object wgpu buffers + upload logic
    │   ├── pipelines.rs           # wgpu RenderPipelines (solid, textured, shader)
    │   ├── solid.wgsl             # WGSL for solid-colour shapes
    │   ├── textured.wgsl          # WGSL for bitmap stimuli
    │   └── shader_stimulus.wgsl   # WGSL template for custom pixel-shader stimuli
    │
    ├── ipc/
    │   ├── mod.rs
    │   ├── zmq_server.rs          # ZeroMQ REP loop → SceneState::handle_request
    │   └── shm_reader.rs          # Shared-memory position reader thread
    │
    ├── input/
    │   ├── mod.rs
    │   ├── mouse.rs               # winit CursorMoved → AnimMousePos
    │   └── gamepad.rs             # gilrs → AnimGamepadPos (feature-gated)
    │
    ├── logging/
    │   ├── mod.rs                 # Event, EventKind, LogLevel, emit! / emit_trace! macros
    │   ├── messenger.rs           # MessengerThread, run_messenger_thread, MessengerConfig
    │   ├── file_writer.rs         # Length-prefixed protobuf file writer (.wllog format)
    │   ├── zmq_pub.rs             # ZMQ PUB socket event publisher (port 5556)
    │   └── sqlite_writer.rs       # Deferred SQLite batch writer (feature = "rusqlite")
    │
    ├── replay/
    │   ├── mod.rs                 # run_replay entry point, timing modes
    │   ├── driver.rs              # ReplayDriver: reads .wllog, dispatches to SceneState
    │   └── log_reader.rs          # Sequential .wllog file reader / deserialiser
    │
    ├── overlay/                   # feature = "overlay"
    │   ├── mod.rs                 # egui context setup, toggle logic
    │   └── panels.rs              # stimulus list, frame graph, ZMQ stats panels, recent events
    │
    ├── bin/
    │   └── convert.rs             # wonderlamp-convert: .wllog → SQLite export tool
    │
    └── proto/                     # generated by build.rs (gitignored)
        └── wonderlamp.rs
```

---

## 7. Phase-by-Phase Implementation Plan

### Phase 3 — Project Scaffolding

- [ ] Update `Cargo.toml` with ZeroMQ / protobuf / tokio / async dependencies (see §4).
- [ ] Write `build.rs`:
  ```rust
  fn main() {
      prost_build::compile_protos(&["proto/wonderlamp.proto"], &["proto/"]).unwrap();
  }
  ```
- [ ] Write `proto/wonderlamp.proto` (schema in §5).
- [ ] Add to `main.rs`:
  ```rust
  pub mod proto {
      include!(concat!(env!("OUT_DIR"), "/wonderlamp.rs"));
  }
  ```
- [ ] Add `args.rs` with `clap` CLI:
  - `--zmq-addr <addr>` (default `tcp://*:5555`)
  - `--display-index <N>` (default `0`)
  - `--fullscreen` flag
  - `--overlay` / `--no-overlay`

### Phase 1 — Scene State Core (`src/scene/`)

> **See `STIMULUS_DATA_MODEL.md` for the full design rationale.** The summary is below.

#### Stimulus representation: enum, not trait objects

Stimuli are **not** modelled as `Box<dyn Stimulus>`. Instead, all stimulus types are variants
of a single `Stimulus` enum, and shared state is held in explicit component structs that every
variant composes. This avoids the pitfalls of mirroring the C++ inheritance hierarchy in Rust.

**Component structs** (shared across all or most variants):

```rust
/// Lifecycle and visibility flags — identical for every stimulus type.
pub struct StimulusFlags {
    pub enabled:      bool,
    pub enabled_copy: bool,
    pub suppressed:   bool,   // toggled by Flicker animation
    pub protected:    bool,   // survives RemoveAll
    pub anim_handle:  Option<u32>,
}

/// 2-D placement with deferred-copy support.
pub struct Transform2D { pub pos: [f32; 2], pub angle: f32 }

/// Fill/outline/stroke appearance with deferred-copy support.
pub struct Appearance {
    pub fill: [f32; 4], pub outline: [f32; 4],
    pub stroke_width: f32, pub draw_mode: DrawMode,
}

/// Generic deferred-mode wrapper: holds a live value and a staging copy.
pub struct Deferred<T: Copy + Default> { pub live: T, pub copy: T }
impl<T: Copy + Default> Deferred<T> {
    pub fn set(&mut self, deferred: bool, value: T) { ... }
    pub fn make_copy(&mut self) { self.copy = self.live; }
    pub fn flip(&mut self)      { self.live = self.copy; }
}
```

**Concrete stimulus structs** — flat, explicit, no base fields:

```rust
pub struct RectStimulus    { pub flags: StimulusFlags, pub transform: Deferred<Transform2D>, pub appearance: Deferred<Appearance>, pub size: Deferred<[f32; 2]> }
pub struct EllipseStimulus { pub flags: StimulusFlags, pub transform: Deferred<Transform2D>, pub appearance: Deferred<Appearance>, pub radii: Deferred<[f32; 2]> }
pub struct PetalStimulus   { pub flags: StimulusFlags, pub transform: Deferred<Transform2D>, pub appearance: Deferred<Appearance>, pub params: Deferred<PetalParams>, pub rebuild: bool }
pub struct WedgeStimulus   { pub flags: StimulusFlags, pub transform: Deferred<Transform2D>, pub appearance: Deferred<Appearance>, pub half_angle: Deferred<f32>, pub rebuild: bool }
// ... etc.
```

**Top-level enum** — heterogeneous collection with no heap allocation per element:

```rust
pub enum Stimulus {
    Rect(RectStimulus),
    Ellipse(EllipseStimulus),
    Petal(PetalStimulus),
    Wedge(WedgeStimulus),
    Disc(DiscStimulus),
    Bitmap(BitmapStimulus),
    BitmapSeq(BitmapSeqStimulus),
    WgslShader(WgslShaderStimulus),
    Particle(ParticleStimulus),
    Pixel(PixelStimulus),
}
```

Shared operations (`move_to`, `make_copy`, `flip`, `is_visible`, `set_anim_param`) are
implemented as methods on the enum, dispatching via `match`. A single `stim_field!` macro
eliminates the boilerplate for accessing common fields:

```rust
macro_rules! stim_field {
    ($stim:expr, |$s:ident| $expr:expr) => {
        match $stim {
            Stimulus::Rect($s) | Stimulus::Ellipse($s) | Stimulus::Petal($s) | ... => $expr,
        }
    };
}
```

The compiler enforces exhaustiveness — a missing variant in `make_copy` or `flip` is a
**compile error**, not a silent bug (unlike the C++ "forgot to call `Super::makeCopy()`"
failure mode).

#### Animation representation: trait objects

Animations remain `Box<dyn Animation>` because their internal state is highly heterogeneous,
the set may be extended, and the per-frame `advance()` interface is uniform regardless of
implementation:

```rust
pub trait Animation: Send + 'static {
    fn advance(&mut self, stimuli: &mut IndexMap<u32, Stimulus>, frame_rate: f32, deferred: bool);
    fn command(&mut self, cmd: &AnimCmd) -> CommandResult;
    fn assign(&mut self, handle: u32);
    fn deassign(&mut self, stimuli: &mut IndexMap<u32, Stimulus>);
    fn stimulus_handle(&self) -> Option<u32>;
    fn final_action(&self) -> FinalActionMask;
    fn set_final_action(&mut self, mask: u8);
}
```

#### `SceneState`

```rust
pub struct SceneState {
    pub stimuli:          IndexMap<u32, Stimulus>,          // no Box, inline storage
    pub animations:       IndexMap<u32, Box<dyn Animation>>,
    pub next_stim_handle: u32,        // starts at 1
    pub next_anim_handle: u32,        // starts at 0x8000
    pub background:       Deferred<[f32; 4]>,
    pub deferred_mode:    bool,
    pub pending_flip:     bool,
    pub photodiode:       PhotoDiodeState,
    pub default_fill:     [f32; 4],
    pub default_outline:  [f32; 4],
    pub frame_rate:       f32,
    pub screen_size:      (u32, u32),
    pub error_mask:       u16,
    pub error_code:       i16,
}
```

Implement `SceneState::handle_request(&mut self, req: Request) -> Response` dispatching the
protobuf `oneof` to the appropriate creation/mutation method.

### Phase 2 — Renderer (`src/render/`)

Refactor the existing `src/main.rs` prototype into the `render/` module.

**Coordinate system:**  
Input coordinates are pixel-space with origin at screen centre, Y-up (visual neuroscience
convention). The vertex shader converts to wgpu NDC:
```wgsl
clip_pos = vec4(pos / (screen_half_size * vec2(1.0, -1.0)), 0.0, 1.0);
```
Pass `screen_half_size` as a push constant or uniform.

**`render/tessellator.rs`** — Keep existing `TessellatedBezier` but make it colour-agnostic.
Add `tessellate_shape(geom: &ShapeGeometry, transform: Affine, fill: [f32;4]) -> (Vec<Vertex>, Vec<u32>)`.

Shape → kurbo mapping:
- `Rect` → `kurbo::Rect` → 4 vertices, 2 triangles
- `Ellipse` → `kurbo::Ellipse::to_path(tolerance)` → centroid fan
- `Petal` → `BezPath`: arc + QuadBez + arc + QuadBez (matching `CPetal::Construct` exactly)
- `Wedge` → `BezPath`: 3 line segments forming a triangle
- `Disc` → `kurbo::Circle::to_path(tolerance)` → centroid fan

**`render/pipelines.rs`** — Two pipelines:
- `solid_pipeline`: vertex colour from buffer (all procedural shapes, photodiode flash)
- `textured_pipeline`: adds UV coords + texture/sampler bind group (bitmaps)
- `shader_pipeline`: custom WGSL fragment shader per stimulus (pixel shader stimuli)

**Render loop (per frame):**
1. Acquire surface texture (blocks on vsync with `PresentMode::Fifo`).
2. Lock `SceneState` (read lock sufficient after the flip).
3. If `pending_flip`: upgrade to write lock, call `get_copy()` on all stimuli, clear flag.
4. Advance all animations: call `anim.advance(stimuli, frame_rate, deferred_mode)`.
5. Begin render pass, clear to `background` colour.
6. Iterate `stimuli` in insertion order (insertion order = draw order):
   - Skip if `!enabled || suppressed`.
   - If `dirty`: re-tessellate, re-upload vertex/index buffers.
   - Encode draw call.
7. Draw photodiode overlay if `photodiode.enabled`.
8. Render egui overlay if `feature = "overlay"` and toggle is on.
9. End render pass, submit, present.
10. Record frame timestamp, update `frame_rate` in `SceneState`.

**`render/gpu_stimulus.rs`** — `GpuBuffers` is owned exclusively by the render thread (no
locking). It mirrors `SceneState::stimuli` by handle:

```rust
pub struct GpuBuffers {
    pub meshes:    HashMap<u32, StimulusMesh>,     // vertex+index per handle
    pub textures:  HashMap<u32, wgpu::Texture>,    // bitmap/shader stimuli
    pub pipelines: Vec<wgpu::RenderPipeline>,      // WGSL shader stimuli
}
pub struct StimulusMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer:  wgpu::Buffer,
    pub index_count:   u32,
}
```

Meshes are rebuilt lazily when `stimulus.needs_rebuild()`. Draw dispatch is a plain `match` on
the `Stimulus` enum — no vtable, no trait objects, fully inlinable per variant.

### Phase 4 — ZeroMQ Server (`src/ipc/zmq_server.rs`)

```rust
pub async fn run_zmq_server(
    scene:     Arc<RwLock<SceneState>>,
    bind_addr: &str,
) -> anyhow::Result<()> {
    let mut socket = zeromq::RepSocket::new();
    socket.bind(bind_addr).await?;
    loop {
        let msg   = socket.recv().await?;
        let bytes = msg.get(0).ok_or(...)?.as_ref();
        let req   = proto::Request::decode(bytes).map_err(...)?;
        let resp  = {
            let mut state = scene.write().unwrap();
            state.handle_request(req)
        };
        let mut out = Vec::new();
        proto::Response::encode(&resp, &mut out)?;
        socket.send(out.into()).await?;
    }
}
```

Spawn with `tokio::spawn` inside a `std::thread::spawn` that creates its own `tokio::Runtime`.

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

Each `WgslShaderStimulus` owns:
- A `wgpu::RenderPipeline` compiled at load time from the user-supplied `.wgsl` file.
- A uniform buffer with layout:
  ```wgsl
  struct ShaderUniforms {
      center: vec2<f32>,
      size:   vec2<f32>,
      params: array<f32, 8>,
      phase:  f32,
      _pad:   vec3<f32>,
  }
  ```
- `phase` is incremented each frame by `phase_inc` (mirrors the C++ `m_phiInc`).

Example port of `Grating.fx`:
```wgsl
@group(0) @binding(0) var<uniform> u: ShaderUniforms;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dist = distance(u.center, pos.xy);
    if dist > u.params[0] { discard; }
    let bright = (sin((pos.x - u.center.x) * 6.2832 / u.params[1] + u.phase) + 1.0) * 0.5;
    return vec4<f32>(bright, bright, bright, 1.0);
}
```

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
| Animation representation | `Box<dyn Animation>` trait object | Heterogeneous internal state; open for extension; uniform `advance()` interface justifies dynamic dispatch |
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

- [ ] **Phase 1** — Scene state: `Stimulus` enum + component structs (see `STIMULUS_DATA_MODEL.md`), `Animation` trait + all variants, `SceneState`
- [ ] **Phase 2** — Renderer: refactor `main.rs` into `render/`, all shape tessellators, pipelines, render loop wired to `SceneState`
- [ ] **Phase 3** — Scaffolding: ZeroMQ / protobuf deps in `Cargo.toml`, `build.rs`, `proto/wonderlamp.proto`, `args.rs`
- [ ] **Phase 4** — ZeroMQ server: `ipc/zmq_server.rs`, wire to `SceneState::handle_request`
- [ ] **Phase 5** — Shared-memory reader: `ipc/shm_reader.rs`, `AnimExternalPos`
- [ ] **Phase 6** — Main + threading: proper fullscreen winit, thread spawning, `Arc<RwLock>`
- [ ] **Phase 7** — Deferred mode: `make_copy`/`get_copy` + `pending_flip` in render loop
- [ ] **Phase 8** — WGSL pixel shader stimuli: runtime pipeline compilation, uniform buffer
- [ ] **Phase 9** — Bitmap / bitmap-sequence stimuli: `image` crate, textured pipeline
- [ ] **Phase 10** — Photodiode sync rectangle
- [ ] **Phase 11** — egui debug overlay (feature-gated)
- [ ] **Phase 12** — Video stimuli via `ffmpeg-next` (deferred, last)
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

The C++ codebase uses a deep inheritance hierarchy (`CStimulus` → `CD2DStimulus` /
`C3DStimulus` → concrete type) to share state and behaviour. **This hierarchy is not ported
to Rust.** Rust has no data inheritance, and mirroring the pattern with `Box<dyn Stimulus>`
would produce worse code: vtable overhead, scattered state, and the same "forgot to call
Super::" silent-failure mode.

Instead, the Rust design uses:

- **Component structs** for shared state (`StimulusFlags`, `Deferred<Transform2D>`,
  `Deferred<Appearance>`) — composed explicitly into each concrete struct.
- **`Deferred<T>`** — a generic wrapper that holds a live value and a staging copy, with
  `.make_copy()` and `.flip()` methods. Replaces the entire `makeCopy()` / `getCopy()`
  virtual chain mechanically and safely.
- **`enum Stimulus`** — a closed enum of all concrete types. Shared operations are `match`
  arms, not virtual methods. The compiler enforces exhaustiveness.
- **`Box<dyn Animation>`** — animations retain trait objects because their state is
  genuinely heterogeneous and the set is open for extension.

See `STIMULUS_DATA_MODEL.md` for the full rationale, all concrete struct definitions, the
`stim_field!` macro, tessellation design, and a comparison table against the C++ design.

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
