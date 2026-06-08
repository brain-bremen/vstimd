# Virtual Trigger Lines (VTL) — Implementation Plan

## Context

vstimd needs to respond to TTL hardware pulses (e.g. from NI-DAQ boards) to
control stimulus visibility and animations at frame granularity. Rather than
embedding DAQ drivers, vstimd defines a **Virtual Trigger Line (VTL)** interface
based on POSIX shared memory. A separate bridge process (e.g. `nidaqd`) maps
real hardware lines onto the shared memory words; vstimd polls them every frame
with zero syscall overhead.

The VTL shm banks always exist once vstimd starts — line *names* are metadata
layered on top. A `SetVtlLineName` command associates a human-readable name
with a (bank, bit, direction) tuple; no "allocation" of lines is needed.

Goals:
- Per-frame trigger-driven animations (couple visibility, pulse on edge, etc.)
- StimServer-style final actions (disable, togglePD, signalEvent, restart, endDeferredMode)
- Output triggers (frame onset, stimulus visible → drive hardware output)
- Photodiode toggling from trigger animations
- No DAQ or hardware dependency inside vstimd
- Debug overlay panel showing VTL state
- Software-trigger ZMQ command for testing without hardware
- Co-start a DAQ bridge subprocess

---

## Design decisions

These questions are resolved.  See `TODO.md` for the full rationale.

**Input / output ownership**

daqd is a facade: its hardware inputs map to vstimd inputs; vstimd outputs map to
daqd hardware outputs.

- Input lines are written by nidaqd (hardware) or ZMQ `SetInput*` (testing).
  **vstimd never writes input lines** — not in animations, not in the render loop.
- Output lines are written by the render loop only.
  ZMQ `SetOutput*` commands exist for **debug / manual override only** and should
  not be used while animations are writing the same bits.

**Animation trigger sources**

Animations may watch **both input and output lines** as trigger sources.  An output
line set by animation A in frame N is visible to animation B at the output-snapshot
step [S] in frame N+1, enabling output-to-output chaining with one frame of latency.

**Animation output commit ordering**

All animations advance in a single pass; none of their outputs are written to shm
until the entire pass is complete.  This prevents animation A from triggering
animation B in the same frame.  The ordering of animations in the `animations` map
is therefore irrelevant for correctness.

**Vblank trigger (special output)**

The "vblank" output line is not driven by an animation.  It is written directly by
the render loop at step [A'] (immediately after input poll, before animations), so
its write latency is minimal.  It is not accumulated via `output_pending`.

---

## New crate: `vtl/`

A small workspace crate (`vtl/`) defining the **POSIX shared memory** layout
(`shm_open` / `mmap` — not System V `shmget`). Both vstimd and future bridge
processes (`nidaqd`, GPIO bridge, etc.) depend on it. nidaqd will be developed
separately; this crate is its contract.

The segment is visible in the filesystem at e.g. `/dev/shm/vstimd_vtl`, which
makes debugging straightforward (`ls -la /dev/shm/`, `hexdump /dev/shm/vstimd_vtl`).

### Layout (`vtl/src/layout.rs`)

```
Header (#[repr(C)], 128 bytes):
  magic:            u32   (0x56544C31 = "VTL1")
  version:          u32   (= 1)
  num_input_banks:  u32   (each bank = 64 lines; start with 1)
  num_output_banks: u32
  seqlock:          AtomicU64   (reserved for future multi-bank consistency; unused in v1)
  pad:              [u8; 96]

Named line table (at fixed offset 128, up to 256 entries = 4 banks × 64 lines):
  n_entries: u32
  pad:       [u8; 60]   (align to 64 bytes)
  entries[256]:
    name:      [u8; 56]   (null-terminated UTF-8)
    bank:      u8
    bit:       u8
    direction: u8         (0=input, 1=output)
    pad:       u8

State section (cache-line-aligned, at offset 4096):
  input_state:      [AtomicU64; 4]   // current levels (DAQ writes, vstimd reads)
  input_rise_latch: [AtomicU64; 4]   // sticky rising edges (fetch_or by DAQ, fetch_and-clear by vstimd)
  input_fall_latch: [AtomicU64; 4]   // sticky falling edges
  output_state:     [AtomicU64; 4]   // vstimd writes levels, nidaqd reads for hardware output
  output_set_pulse: [AtomicU64; 4]   // one-shot pulses: vstimd OR-sets, nidaqd clears after driving hardware
```

Start with `MAX_BANKS = 4` (256 lines max). v1 always uses 1 input bank and 1
output bank; the extra banks are reserved. The seqlock is in the header for
future multi-bank consistency but is not read or written in v1.

**Why atomic latches instead of pure level polling:** a trigger pulse shorter
than one frame (~1–5 ms vs. ~16 ms at 60 Hz) will be gone from `input_state`
before vstimd wakes up. `input_rise_latch` / `input_fall_latch` accumulate edges
set by nidaqd via `fetch_or(bit)`; vstimd clears consumed bits with
`fetch_and(!bit)` each frame. This guarantees no edge is missed regardless of
pulse width.

**Software triggers write to shared memory:** `SetVtlLine` (via ZMQ) writes to
the same `input_rise_latch` / `input_state` atomics that nidaqd would write to.
There is no internal bypass — the shm is the unified interface regardless of
whether a trigger comes from hardware or software.

### Ownership split (`vtl/src/`)

- `owner.rs` — `VtlOwner`: `shm_open(O_CREAT|O_RDWR)` + `ftruncate` + `mmap`,
  writes header + initial named-line table, `shm_unlink` on drop. Used by vstimd.
- `client.rs` — `VtlClient`: `shm_open(O_RDWR)` (existing) + `mmap`. Used by
  nidaqd and the test tool. Does not unlink on drop.

Both expose the same `AtomicU64` field accessors. All cross-process access goes
through `Relaxed`/`Release`/`Acquire` atomics — no mutex needed.

A `vtl/examples/vtl-test-client.rs` binary opens an existing segment as a
client and lets you toggle lines by name from the CLI, useful for testing without
hardware.

---

## New module: `server/src/scene/vtl_state.rs`

`VtlState` wraps `VtlOwner` and adds per-frame edge detection.

```rust
pub struct VtlEdges {
    pub rising:  [u64; MAX_BANKS],
    pub falling: [u64; MAX_BANKS],
    pub current: [u64; MAX_BANKS],
}

pub struct VtlState {
    owner:       VtlOwner,
    prev_input:  [u64; MAX_BANKS],  // render-thread only, input level tracking
    prev_output: [u64; MAX_BANKS],  // render-thread only, output level tracking for edge detection
}

impl VtlState {
    /// Called once per frame from render thread at [A].
    /// Drains the input latches and returns edges seen since the last frame.
    pub fn poll(&mut self) -> VtlEdges { ... }

    /// Called at [S] — returns a snapshot of current output_state for animation
    /// trigger detection.  Animations use this to detect edges on output lines
    /// (comparing against prev_output, which is updated here).
    pub fn output_snapshot(&mut self) -> [u64; MAX_BANKS] { ... }

    /// Write output_state to shm.  Called twice per frame:
    ///   [A'] vblank trigger (one bit, before animations)
    ///   [C]  animation output commit (output_pending, after all animations)
    pub fn write_outputs(&self, state: &[u64; MAX_BANKS]) { ... }

    /// Software trigger from ZMQ thread — writes input_rise_latch or input_fall_latch.
    /// Thread-safe: only uses fetch_or on an AtomicU64.
    pub fn software_trigger(&self, bank: usize, bit: u8, edge: Edge) { ... }
}
```

`VtlState` is wrapped in `Arc<Mutex<VtlState>>` so both the render thread
(via `poll`) and the ZMQ thread (via `software_trigger` / `SetVtlLine` command)
can access it without a race.

---

## New module: `server/src/scene/animation.rs`

### Final action (from StimServer `CAnim::m_finalAction`)

```rust
bitflags! {
    pub struct FinalAction: u8 {
        const DISABLE           = 0x01;
        const TOGGLE_PHOTODIODE = 0x04;
        const SIGNAL_EVENT      = 0x08;   // fires a named output VTL line one pulse
        const RESTART           = 0x10;
        const REVERSE           = 0x20;
        const END_DEFERRED      = 0x80;
    }
}
```

`Finalize(entry)` in `advance_animations`:
- `DISABLE` → set `stimulus.enabled = false`
- `TOGGLE_PHOTODIODE` → `scene.photodiode.lit ^= true`
- `SIGNAL_EVENT` → set a configured output VTL bit (1-frame pulse)
- `RESTART` → reset animation to start, stay `Running`
- `END_DEFERRED` → call `scene.end_deferred()` (clears pending_flip)

> **One-frame gap with Python-mediated handoff.**
> If a Python script watches a `SIGNAL_EVENT` output bit and responds by enabling
> a second stimulus (via ZMQ), there will always be a one-frame gap: the bit is
> committed at [C] (after present), the Python ZMQ round-trip takes some ms, and
> even in the best case the `SetEnabled` command arrives during frame N+1
> tessellation.  Stimulus B therefore appears no earlier than vblank N+2 while
> stimulus A disappeared at vblank N+1.
>
> **Possible future workaround — pre-final trigger:** fire a separate output bit
> at frame N-1 (one frame before completion) so Python has time to enable B
> before the final frame.  B then appears from frame N onward (overlap with A's
> last frame) rather than after a gap.  A dedicated animation parameter — e.g.
> `pre_final_output: Option<VtlBit>` on `TriggerFlash` / `Flash` — would make
> this explicit.  Not yet implemented.

### Animation variants

Trigger-reactive (wait for a VTL edge or level):
```rust
CoupleVisibility   { vtl_bank: u8, vtl_bit: u8, polarity: Polarity, stimulus: u32 }
EdgeSetEnabled     { vtl_bank: u8, vtl_bit: u8, edge: Edge, stimulus: u32, enabled: bool }
TriggerFlash       { vtl_bank: u8, vtl_bit: u8, edge: Edge, stimulus: u32,
                     duration_frames: u32, final_action: FinalAction }
TriggerFlicker     { vtl_bank: u8, vtl_bit: u8, edge: Edge, stimulus: u32,
                     on_frames: u32, off_frames: u32,
                     total_frames: Option<u32>, final_action: FinalAction }
```

Free-running (not trigger-gated, start when Armed):
```rust
Flash    { stimulus: u32, duration_frames: u32, final_action: FinalAction }
Flicker  { stimulus: u32, on_frames: u32, off_frames: u32,
           total_frames: Option<u32>, final_action: FinalAction }
Harmonic { stimulus: u32, amplitude: f32, phase_inc: f32, direction_deg: f32,
           final_action: FinalAction }
LinearRange { stimulus: u32, param: AnimParam, start: f32, end: f32,
              duration_frames: u32, final_action: FinalAction }
ExternalPosition { stimulus: u32, shm_name: String, x_offset: f32, y_offset: f32 }
  // Opens a separate named POSIX shm containing [f32; 2] (x, y).
  // Mirrors CAnimExternalPositionControl / Windows MapViewOfFile pattern.
  // Each frame: stim.x = shm[0] + x_offset, stim.y = shm[1] + y_offset.
  // The writer (motion tracker, etc.) owns the segment; vstimd opens O_RDONLY.
```

Output (vstimd → hardware):
```rust
FrameOnsetOutput   { vtl_bank: u8, vtl_bit: u8, pulse_frames: u32 }
StimulusVisibleOut { vtl_bank: u8, vtl_bit: u8, stimulus: u32 }
```

`AnimParam` covers: `PositionX`, `PositionY`, `Alpha`, `GratingPhase`,
`GratingContrast`, `GratingSf`.

### External float shared memory

`ExternalPosition` uses a **separate** named POSIX shm — not the VTL segment.
The VTL shm is for bit-packed digital trigger lines only. A position-control shm
at an arbitrary path (e.g. `/vstimd_pos_myobj`) contains a flat `[f32; N]`
array. vstimd opens it read-only at animation creation time and reads it each
frame with a pointer load (no atomic needed — a torn f32 read is acceptable for
position data; worst case is one stale frame). The mmap'd pointer is stored in
`AnimationEntry`. On `DeleteAnimation` the mmap is unmapped.

This is the natural extension point for **camera/VR motion control**: a separate
`ExternalCameraControl` animation (or a dedicated scene command) would open a
float shm containing a 6-DOF pose `[f32; 7]` (position + quaternion) and update
a virtual camera transform each frame. The exact layout is TBD when camera
support is designed, but the shm mechanism is identical.

### State machine

```rust
pub enum AnimState {
    Idle,
    Armed,
    Running { frame_counter: u32 },
    Done,
}
```

Only `Armed` animations react to triggers. `CoupleVisibility` and output
animations are `Running` permanently once armed. The Python experiment
controller calls `ArmAnimation(handle)` at trial start.

### SceneState additions (`scene/state.rs`)

```rust
pub animations: IndexMap<u32, AnimationEntry>,
pub next_anim_handle: u32,
pub vtl_names: HashMap<String, (u8, u8, Direction)>,  // name → (bank, bit, dir)
```

VTL names are stored in `SceneState` and written to the shm named-line table on
change. They are the source of truth; the shm table is a copy for bridge processes.

---

## `SceneState::advance_animations`

Called from render thread while write lock is held (right after `apply_flip`).

```rust
pub fn advance_animations(
    &mut self,
    input_edges: &VtlEdges,          // from VtlState::poll() at [A]
    output_snapshot: &[u64; MAX_BANKS], // snapshot of output_state read at [S]
    output_pending: &mut [u64; MAX_BANKS], // accumulated; written to shm at [C]
)
```

`output_snapshot` is a frozen copy of the output shm read **before** any animation
runs.  It serves two purposes:
1. Edge detection on output lines (for animation-to-animation chaining).
2. Level/polarity tests for `CoupleVisibility`-style output-watching animations.

Trigger animations may watch either `input_edges` (input lines) or derive edges
from `output_snapshot` vs the previous frame's output (output lines).  For input
lines the latch-based edges from `poll()` are authoritative (sub-frame pulses not
missed).  For output lines a simple level comparison against `prev_output` in
`VtlState` is sufficient because output changes are always frame-aligned.

Per-animation logic:
- `CoupleVisibility`: `stim.enabled = (snapshot[bank] >> bit) & 1 == polarity`
  (if watching an output line; else uses `input_edges.current`)
- `EdgeSetEnabled`: on matching edge in input latch or output level change → set enabled
- `TriggerFlash`: on edge → `Running{frame_counter: duration_frames}`, enable stim;
  decrement each frame; on 0 → `Finalize`
- `TriggerFlicker`: on edge → start on/off counter cycling; `Finalize` after total_frames
- `Flash`, `Flicker`, `Harmonic`, `LinearRange`: fire on `Armed` immediately (no trigger)
- `FrameOnsetOutput`: always `Running`; set output bit in `output_pending` each frame
- `StimulusVisibleOut`: mirror `stim.enabled` into `output_pending`

All output bits are accumulated into `output_pending` — **nothing is written to shm
during the loop**.  The render loop calls `vtl_state.write_outputs(output_pending)`
once at [C], after all animations have advanced.  This single-commit step is what
prevents same-frame ordering effects.

---

## Proto additions (`proto/vstimd/v1/vtl.proto`)

**VTL line naming (names are metadata, not allocation):**
```proto
SetVtlLineName   { bank, bit, direction, name }  // "" clears the name
ListVtlLines     {} → [{ name, bank, bit, direction, current_value }]
SetVtlLine       { bank, bit, value }             // software trigger / clear; writes shm directly
```

Lines can also be addressed by name in all commands
(`oneof target { BankBit bank_bit = 1; string name = 2; }`).

**Animation management:**
```proto
CreateAnimation  { name, final_action_mask: uint32, oneof body { ... } } → handle
ArmAnimation     { handle }
DisarmAnimation  { handle }
DeleteAnimation  { handle }
ListAnimations   {} → [{ handle, name, state, type_name }]
```

Animation `body` oneof covers all `Animation` variants.

Added to `Request.command` oneof and dispatched in `scene/command.rs`.

---

## Frame loop integration (`render/vk/frame.rs`)

`pending_outputs: [u64; MAX_BANKS]` is stored in the backend struct and carried
across frame boundaries.  At the very start of the frame loop iteration (in DRM
mode: immediately after `wait_vblank()` returns; in winit mode: after
`vkWaitForPresentKHR` confirms the previous present) the outputs from the
PREVIOUS frame's animation pass are committed.  This aligns the hardware outputs
with actual display scan-out rather than with GPU submission time.

```rust
// ── Top of frame loop, immediately after vblank wait ────────────────────────

let vblank_mask = vblank_trigger_bit.map_or([0u64; MAX_BANKS], |(bank, bit)| {
    let mut m = [0u64; MAX_BANKS];
    m[bank] |= 1u64 << bit;
    m
});

if let Some(vtl) = vtl_state.as_ref() {
    let mut vtl = vtl.lock().unwrap();

    // [A] Commit previous frame's animation outputs + raise vblank trigger.
    //     In DRM mode this fires within microseconds of the hardware scan-out
    //     flip, aligning animation outputs with actual display visibility.
    //     The vblank trigger bit is ORed in here and cleared at [C].
    let commit: [u64; MAX_BANKS] = std::array::from_fn(|i| pending_outputs[i] | vblank_mask[i]);
    vtl.write_outputs(&commit);

    // [A] Poll input edges (drains rise/fall latches).
    let input_edges = vtl.poll();

    // [S] Snapshot output_state (includes animation outputs + vblank HIGH).
    let output_snapshot = vtl.output_snapshot();

    // Animation pass — all animations advance; none of their outputs reach shm
    // until the next [A].  pending_outputs is reset to accumulate this frame.
    pending_outputs = [0u64; MAX_BANKS];
    sc.advance_animations(&input_edges, &output_snapshot, &mut pending_outputs);
}

// ... tessellate / record / submit / present ...

// [C] Clear vblank trigger.  Animation outputs (pending_outputs_prev) remain.
//     Pulse width [A]→[C] = vstimd's active compute time for this frame.
if let Some(vtl) = vtl_state.as_ref() {
    // pending_outputs_prev is the animation output state committed at [A].
    // Re-write it without the vblank bit to pull the trigger LOW.
    vtl.lock().unwrap().write_outputs(&pending_outputs_prev);
}
pending_outputs_prev = pending_outputs;  // save for next iteration's [A]
```

`vtl_state: Option<Arc<Mutex<VtlState>>>` is threaded through from the backend
entry points (`render/drm/mod.rs`, `render/winit_vk/mod.rs`), which create it
at startup when a VTL shm path is available (always on Linux, configurable path).

Note on winit mode: `vkWaitForPresentKHR` confirms GPU completion of the previous
present, not the hardware scan-out flip.  The [A] write therefore lands slightly
before the actual vblank in winit mode.  DRM mode (`wait_vblank()`) gets true
hardware vblank accuracy for both the rising and falling edges of the vblank trigger.

The render state needs two persistent fields across frames:
- `pending_outputs: [u64; MAX_BANKS]` — animation outputs accumulating this frame
- `pending_outputs_prev: [u64; MAX_BANKS]` — committed at [A], cleared at [C]
- `vblank_trigger_bit: Option<(usize, u8)>` — configured bank/bit, or `None`

---

## Debug overlay (`render/overlay.rs`)

New collapsible "VTL Lines" panel:
- Two sub-tables: Input lines | Output lines
- Columns: name | bank/bit | current level (green/grey dot) | edge count since last reset
- "Reset counters" button
- Per input line: "Fire rising" / "Fire falling" buttons (calls `software_trigger`, writes shm)

---

## Co-start subprocess (`main.rs`)

CLI: `--vtl-subprocess <binary> [args...]` (or configured in a future TOML config).

After creating `VtlOwner`, vstimd sets `VSTIMD_VTL_SHM=<path>` in the
environment and spawns the binary. The child process is stored and checked on
exit; vstimd logs a warning if it exits unexpectedly but does not restart it
(restart is a supervisor's responsibility).

---

## Implementation steps

Each step produces a building, tested vstimd. Review and merge before starting
the next one.

---

### Step 1 — `vtl/` crate (standalone, no vstimd changes)

Create `vtl/` as a new workspace crate with:
- `src/layout.rs` — `#[repr(C)]` structs and `AtomicU64` state section
- `src/owner.rs` — `VtlOwner` (`shm_open O_CREAT`, `ftruncate`, `mmap`, `shm_unlink` on drop)
- `src/client.rs` — `VtlClient` (`shm_open` existing, `mmap` only)
- `tests/layout.rs` — create owner, write bits from a second thread simulating nidaqd,
  read back edges, confirm latches clear
- `examples/vtl-test-client.rs` — CLI: open existing shm by path, read/toggle lines by name

**Review gate:** `cargo test -p vtl`, run vtl-test-client standalone.

---

### Step 2 — VTL plumbing in vstimd (no animations)

- `server/src/scene/vtl_state.rs` — `VtlState` / `VtlEdges`, `poll`, `write_outputs`, `software_trigger`
- `server/src/scene/state.rs` — add `vtl_names: HashMap<String, (u8, u8, Direction)>`
- `proto/vstimd/v1/vtl.proto` — `SetVtlLineName`, `ListVtlLines`, `SetVtlLine` only (no animation messages yet)
- `server/src/scene/command.rs` — dispatch those three commands
- `server/src/render/vk/frame.rs` — add VTL poll hook (calls `poll` + `write_outputs`; advance_animations is a no-op placeholder)
- `render/drm/mod.rs` + `render/winit_vk/mod.rs` — create `VtlOwner` at startup, wrap in `Arc<Mutex<VtlState>>`
- `server/src/ipc.rs` — thread `Arc<Mutex<VtlState>>` through for `SetVtlLine`
- `render/overlay.rs` — VTL Lines panel (name, bank/bit, level dot, edge counters, fire buttons)

**Review gate:** start vstimd, open overlay, see VTL panel with no lines; register
two lines via Python (`SetVtlLineName`); fire a rising edge via `SetVtlLine`;
confirm level indicator updates; confirm vtl-test-client sees the same bit state.

---

### Step 3 — Animation framework skeleton

Wire in data structures and proto API; `advance_animations` iterates but applies
no effects yet.

- `server/src/scene/animation.rs` — `Animation` enum (all variants as stubs), `AnimState`,
  `FinalAction` bitflags, `AnimationEntry`
- `server/src/scene/state.rs` — add `animations: IndexMap<u32, AnimationEntry>`,
  `next_anim_handle`; stub `advance_animations`
- `proto/vstimd/v1/vtl.proto` — add `CreateAnimation`, `ArmAnimation`, `DisarmAnimation`,
  `DeleteAnimation`, `ListAnimations`
- `server/src/scene/command.rs` — dispatch animation management commands

**Review gate:** create, list, arm, delete animations via Python client; verify
`ListAnimations` returns correct state; no stimulus changes occur yet.

---

### Step 4 — Core trigger animations

Implement the primary neuroscience patterns and basic final actions.

Animations: `CoupleVisibility`, `EdgeSetEnabled`, `TriggerFlash`

Final actions: `DISABLE`, `TOGGLE_PHOTODIODE`, `RESTART`, `END_DEFERRED`

**Review gate:** Python script creates `TriggerFlash(vtl_line, stimulus, duration=5)`,
arms it, fires `SetVtlLine` rising edge, observes stimulus enabled for 5 frames
then disabled. Test `TOGGLE_PHOTODIODE`: confirm photodiode flips in overlay.
Test `RESTART`: confirm animation re-fires on every trigger.

---

### Step 5 — Output triggers + SIGNAL_EVENT final action

Animations: `FrameOnsetOutput`, `StimulusVisibleOut`

Final action: `SIGNAL_EVENT` (sets a configured output VTL bit for 1 frame)

**Review gate:** arm `FrameOnsetOutput`, open vtl-test-client, confirm output bit
pulses every frame. Arm `StimulusVisibleOut`, toggle stimulus enabled, confirm
output bit mirrors it. Test `SIGNAL_EVENT` final action on a `TriggerFlash`.

---

### Step 6 — Free-running and flicker animations

Animations: `Flash`, `Flicker`, `TriggerFlicker`

**Review gate:** arm `Flash(stimulus, 10 frames)`, confirm stimulus lights for 10
frames then disables. Arm `Flicker(on=3, off=2)`, confirm cycling in overlay.
Test `TriggerFlicker`: trigger starts the flicker cycle.

---

### Step 7 — Continuous animations (Harmonic, LinearRange)

Animations: `Harmonic`, `LinearRange`

**Review gate:** arm `Harmonic(stimulus, amplitude=100, phase_inc=0.05)`, observe
sinusoidal position motion. Arm `LinearRange(stimulus, PositionX, 0→400, 120 frames)`,
confirm smooth travel.

---

### Step 8 — ExternalPosition (float shm)

- `ExternalPosition` animation: `shm_open O_RDONLY` at animation creation time,
  store mmap'd `*const f32` in `AnimationEntry`, unmap on delete
- `server/src/scene/command.rs` — `CreateAnimation` with `ExternalPosition` variant

**Review gate:** small Python script creates a POSIX shm (`/dev/shm/mypos`) with
two `f32` values; create `ExternalPosition` animation pointing at it; write new
coordinates from Python; observe stimulus moving each frame.

---

### Step 9 — Co-start subprocess

- `server/src/main.rs` — `--vtl-subprocess <binary> [args...]` flag; after creating
  `VtlOwner`, set `VSTIMD_VTL_SHM` env var and `Command::spawn`; log warning on child exit

**Review gate:** pass `--vtl-subprocess echo hello`; confirm "hello" appears in
log output at startup. Pass a real nidaqd stub; confirm it receives the shm path.

---

## Files to create / modify

| File | Change |
|---|---|
| `vtl/` (new crate) | `Cargo.toml`, `src/lib.rs`, `src/layout.rs`, `src/owner.rs`, `src/client.rs`, `examples/vtl-test-client.rs` |
| `Cargo.toml` (workspace) | add `vtl` member |
| `server/Cargo.toml` | add `vtl` + `bitflags` dependencies |
| `proto/vstimd/v1/vtl.proto` | VTL name + animation messages |
| `server/build.rs` | include new proto file |
| `server/src/scene/animation.rs` | new — `Animation`, `AnimationEntry`, `AnimState`, `FinalAction` |
| `server/src/scene/vtl_state.rs` | new — `VtlState`, `VtlEdges` |
| `server/src/scene/mod.rs` | export new modules |
| `server/src/scene/state.rs` | add `animations`, `vtl_names` fields; `advance_animations` |
| `server/src/scene/command.rs` | dispatch new proto messages |
| `server/src/render/vk/frame.rs` | poll VTL + advance animations each frame |
| `server/src/render/drm/mod.rs` | create/pass `VtlState` |
| `server/src/render/winit_vk/mod.rs` | create/pass `VtlState` |
| `server/src/render/overlay.rs` | VTL panel |
| `server/src/ipc.rs` | pass `Arc<Mutex<VtlState>>` for `SetVtlLine` handler |
| `server/src/main.rs` | co-start subprocess logic |

---

## Verification

1. **Unit tests** (`vtl/tests/`): create `VtlOwner`, write bits from a thread simulating nidaqd, read back edges with `poll()`, confirm latch clears.
2. **Integration tests** (null renderer): via ZMQ, set a VTL line name, create a `TriggerFlash` animation, fire `SetVtlLine` software trigger, assert stimulus `enabled` changes correctly across `advance_animations`.
3. **Final action test**: animation with `TOGGLE_PHOTODIODE | RESTART` — confirm photodiode toggles each time flash completes and animation re-arms.
4. **Overlay**: run desktop mode, open VTL panel, use "Fire rising" button, confirm level indicator updates.
5. **External client**: run `vtl-test-client` while vstimd runs; toggle bits and verify overlay reflects the changes.
6. **Output ordering test** (integration): create two animations: A (`TriggerFlash` triggered by input line, `SIGNAL_EVENT` final action → output bit X) and B (`TriggerFlash` triggered by output bit X).  Fire the input trigger in frame N.  Assert:
   - Animation A becomes `Running` in frame N.
   - Animation B does NOT become `Running` until frame N+1 (output bit X not visible until next snapshot).
   - This confirms the single-commit rule prevents same-frame cascades.
7. **Input immutability test** (unit): call `advance_animations` with any animation configuration; verify that no input shm bits (`input_state`, `input_rise_latch`, `input_fall_latch`) are modified.
8. **Vblank trigger timing test** (integration): with a vblank output bit configured, assert the bit is present in `output_snapshot` at [S] in the frame following the vblank write (i.e., animations in that frame can detect the rising edge on the vblank line).
