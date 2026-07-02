# Virtual Trigger Lines (VTL) — Implementation Plan

## Context

vstimd needs to respond to TTL hardware pulses (e.g. from NI-DAQ boards) to
control stimulus visibility and animations at frame granularity. Rather than
embedding DAQ drivers, vstimd defines a **Virtual Trigger Line (VTL)** interface
based on POSIX shared memory. A separate bridge process (e.g. `daqd`) maps
real hardware lines onto the shared memory words; vstimd polls them every frame
with zero syscall overhead.

The VTL shm banks always exist once vstimd starts — line *names* are metadata
layered on top. A `SetVirtualTriggerLineName` command associates a human-readable name
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

**Input / output ownership**

daqd is a facade: its hardware inputs map to vstimd inputs; vstimd outputs map to
daqd hardware outputs.

- Input lines are written by daqd (hardware) or ZMQ `SetVirtualTriggerLine`
  (kind=INPUT, testing).
  **vstimd never writes input lines** — not in animations, not in the render loop.
- Output lines are written by the render loop only.
  ZMQ `SetVirtualTriggerLine` (kind=OUTPUT) commands exist for **debug / manual
  override only** and should not be used while animations are writing the same bits.

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

**Two-layer stimulus visibility: `user_enabled` + `anim_enabled`**

`StimulusFlags` carries two independent visibility bits:
- `enabled` — written by `SetEnabled`/`SetAllEnabled` (ZMQ thread / user commands)
- `anim_enabled` — written by the render thread (animations); defaults to `true`

A stimulus is visible only when both are `true`.  This prevents animations and user
commands from silently overriding each other:
- `CoupleVisibilityToInputTriggerLine`, `FlickerForNFrames` write `anim_enabled` each frame
- `FlashForNFrames`, `EnableOnTriggerEdge` write `user_enabled` (permanent state changes)
- `anim_enabled` is **not** part of deferred mode — the render thread owns it exclusively

**Unified trigger-gated / free-running design**

The original plan had separate `TriggerFlash`/`Flash` and `TriggerFlicker`/`Flicker`
variants.  These are now unified: every animation has an optional `start_trigger` field
in `CreateAnimationRequest`.  If absent the animation starts immediately when armed;
if present it waits for the specified edge on a VTL input line before starting.

**Multiple stimuli per animation**

`CreateAnimationRequest.stimuli: repeated uint32` allows one animation to control
any number of stimuli simultaneously.  The original StimServer had one animation per
stimulus (`m_pStimulus`).

**`RESTORE_STATE` final action**

`FinalAction::RESTORE_STATE` (0x40) captures `user_enabled` for each controlled
stimulus at the moment the animation first transitions to Running, then restores it
on completion.  Allows "flash and restore" without needing to know the prior state.

---

## Animation variants (current)

All variants live in `server/src/scene/animation.rs`.  `start_trigger: Option<(VtlBit, Edge)>`
and `stimuli: Vec<u32>` live on `AnimationEntry`, not on the variant.

```
Visibility-coupled (runs indefinitely):
  CoupleVisibilityToInputTriggerLine { trigger: VtlBit, polarity: bool }
    writes anim_enabled each frame: anim_enabled = (input_level == polarity)

One-shot on trigger edge:
  EnableOnTriggerEdge { trigger: VtlBit, edge: Edge, enabled: bool }
    writes user_enabled = enabled once when the edge fires, then Done

Timed visibility:
  FlashForNFrames { duration_frames: u32 }
    sets user_enabled=true at start, Done after N frames; DISABLE sets user_enabled=false
  FlickerForNFrames { on_frames, off_frames, total_frames: Option<u32>, start_on_phase: bool }
    alternates anim_enabled on/off; Done after total_frames (or runs forever)

Motion:
  MoveAlongPath2D { coords: Vec<[f32; 2]> }
    plays back preloaded position sequence, one point per frame
  MoveAlongSegments2D { waypoints: Vec<[f32; 2]>, speed_px_per_sec: f32 }
    piecewise-linear motion at constant speed

External input:
  ExternalPosition2D { shm_name: String, x_offset: f32, y_offset: f32 }
    reads 2-D position from a POSIX shm float array each frame
```

**Final actions** (`FinalAction` bitflags, executed when animation transitions to Done):
```
DISABLE                 (0x01)  set user_enabled=false for all stimuli
TOGGLE_PHOTODIODE       (0x04)  toggle photodiode lit state
FINAL_ACTION_TRIGGER_LINE (0x08)  set configured output VTL bit for one frame
RESTART                 (0x10)  reset to Running { frame_counter: 0 }
REVERSE                 (0x20)  reserved
RESTORE_STATE           (0x40)  restore user_enabled snapshot captured at start
END_DEFERRED            (0x80)  call end_deferred_mode
```

**Not carried forward from StimServer** (may be added later):
- `Harmonic` — sinusoidal oscillation (A·sin(φ), φ increments per frame)
- `LinearRange` / `IntegerRange` — linearly interpolate any numeric parameter
- `initialState` final action (equivalent to RESTORE_STATE above)
- `FrameOnsetOutput`, `StimulusVisibleOut` — output-driving animations

---

## Implementation status

### ✅ Step 1 — `vtl/` crate
`vtl/src/`: `layout.rs`, `owner.rs`, `client.rs`, `segment.rs`, `lib.rs`.
`VtlOwner` creates/manages the shm segment; `VtlClient` attaches read-only.

### ✅ Step 2 — VTL plumbing in vstimd
- `server/src/vtl_state.rs` — `VtlState`, `VtlEdges`, `poll`, `output_edges`, `commit_staged`
- `proto/vstimd/v1/vtl.proto` — `SetVirtualTriggerLineName`, `ListVirtualTriggerLines`, `SetVirtualTriggerLine`, `ToggleVirtualTriggerLine` commands
- `server/src/scene/command.rs` — VTL commands dispatched
- `server/src/ipc.rs` — `Arc<Mutex<VtlState>>` threaded through to handle_request
- `render/drm/mod.rs` + `render/winit_vk/mod.rs` — `VtlState` created at startup, stored as `Option<Arc<Mutex<VtlState>>>`

**Remaining in Step 2:**
- ~~Frame loop VTL poll/write_outputs (currently commented out)~~ → moved into Step 4
- Overlay VTL Lines panel → Step 5

### ✅ Step 3 — Animation framework skeleton
- `server/src/scene/animation.rs` — all variants, `AnimState`, `FinalAction`, `AnimationEntry`
- `server/src/scene/state.rs` — `animations: IndexMap`, `advance_animations` (stub)
- `proto/vstimd/v1/animations.proto` — full proto API including `QueryAnimation`
- `server/src/scene/command.rs` — create/arm/disarm/delete/list/query animations dispatched
- `client/python/vstimd/animations/` — `AnimationClient`, `FinalAction`, `AnimationDetails`

**Proto API changes vs original plan:**
- `start_trigger` + `start_edge` moved to `CreateAnimationRequest` (all variants share it)
- `stimuli: repeated uint32` on `CreateAnimationRequest` (not per-variant)
- `TriggerFlash`/`Flash` merged into `FlashForNFrames`; `TriggerFlicker`/`Flicker` into `FlickerForNFrames`
- `SIGNAL_EVENT` renamed to `FINAL_ACTION_TRIGGER_LINE`; field renamed `final_action_trigger_line`
- `RESTORE_STATE` (0x40) added
- `MoveAlongPath2D`, `MoveAlongSegments2D` added
- `FlickerForNFrames.total_frames`: proto3 `optional uint32` (absent = infinite; was 0-sentinel)
- `FlickerForNFrames.start_on_phase`: bool, allows starting in off-phase
- `AnimationEntry.captured_user_enabled`: Option<Vec<bool>> for RESTORE_STATE
- `QueryAnimationRequest`/`QueryAnimationResponse` added

---

## Remaining steps

### Step 4 — Frame loop integration + core animation execution ← **NEXT**

Wire VTL into both frame loops and implement `advance_animations` for
the core visibility variants and all final actions.

**Frame loop changes** (`render/drm/mod.rs`, `render/winit_vk/mod.rs`):
- Add `pending_outputs: [u64; MAX_BANKS]` and `pending_outputs_prev: [u64; MAX_BANKS]`
  to backend structs.
- At [A] (after vblank wait):
  1. `vtl.write_outputs(&pending_outputs_prev)` — commit previous frame's animation outputs
  2. `let input_edges = vtl.poll()` — drain input latches
  3. `let output_edges = vtl.output_edges()` — compute output edges for trigger detection
  4. `scene.advance_animations(&input_edges, &output_edges, &mut pending_outputs)` —
     advance all animations (brief write lock on scene, released before render)
- At [C] (after present): `pending_outputs_prev = std::mem::take(&mut pending_outputs)`

**`advance_animations` implementation** (`scene/state.rs`):
- `CoupleVisibilityToInputTriggerLine`: set `anim_enabled` from input level each frame
- `EnableOnTriggerEdge`: watch for edge on `trigger`; write `user_enabled` once; Done
- `FlashForNFrames`: set `user_enabled=true` at start; Done after N frames
- `FlickerForNFrames`: alternate `anim_enabled`; Done after `total_frames` if set
- `start_trigger` logic: Armed → Running when specified edge fires (or immediately if absent)
- All `FinalAction` bits: DISABLE, TOGGLE_PHOTODIODE, FINAL_ACTION_TRIGGER_LINE, RESTART,
  RESTORE_STATE, END_DEFERRED

**Review gate:** Python script:
1. Creates a `FlashForNFrames(5 frames, DISABLE)`, arms it, confirms stimulus appears
   for 5 frames then disappears.
2. Creates `FlickerForNFrames(on=3, off=2, total=30)`, arms it, confirms cycling.
3. Creates `FlashForNFrames(5 frames)` with `start_trigger=(0,0,Rising)`, arms it,
   fires `SetVirtualTriggerLine` (kind=INPUT) rising edge, confirms stimulus shows for 5 frames.
4. Creates `CoupleVisibilityToInputTriggerLine`, arms it, toggles input line, confirms
   stimulus tracks it.

---

### Step 5 — Overlay VTL panel + disarm cleanup

- `render/overlay.rs` — "VTL Lines" panel: input/output sub-tables, name/bank/bit/level dot,
  edge counters, "Fire rising"/"Fire falling" buttons (calls `software_trigger`)
- `cmd_disarm_animation`: reset `anim_enabled=true` for animations that write `anim_enabled`
  (CoupleVisibilityToInputTriggerLine, FlickerForNFrames) when returning to Idle

**Review gate:** open overlay, see VTL panel; use Fire buttons; confirm level indicator
and edge counts update.

---

### Step 6 — Motion animations

Implement `MoveAlongPath2D` and `MoveAlongSegments2D` in `advance_animations`:
- `MoveAlongPath2D`: each frame, set stimulus position to `coords[frame_counter]`; Done when exhausted
- `MoveAlongSegments2D`: compute per-segment step count from `speed_px_per_sec` and `measured_fps`; interpolate; Done when last waypoint reached

**Review gate:** arm `MoveAlongPath2D` with a circular path, confirm stimulus traces it.
Arm `MoveAlongSegments2D` with two waypoints, confirm smooth travel at specified speed.

---

### Step 7 — ExternalPosition2D

- Open POSIX shm `O_RDONLY` at animation creation time, store raw `*const f32` pointer
- Read `[0]`/`[1]` each frame (relaxed load; torn read is acceptable for position)
- Unmap on `DeleteAnimation`

**Review gate:** Python writes x/y to `/dev/shm/mypos`, stimulus tracks it each frame.

---

### Step 8 — Co-start subprocess

`--vtl-subprocess <binary> [args...]` in `main.rs`:
- After `VtlOwner` creation, set `VSTIMD_VTL_SHM=<path>` and `Command::spawn`.
- Log warning on unexpected child exit; do not restart.

**Review gate:** `--vtl-subprocess echo hello` → "hello" in log.

---

## Frame loop timing reference

```
── vblank N fires ──────────────────────────────────────────────────────────
  (DRM: wait_vblank returns; winit: vkWaitForPresentKHR confirms frame N-1 visible)

  [A] vtl.write_outputs(pending_outputs_prev)   ← commit frame N-1 animation outputs
  [A] input_edges = vtl.poll()                  ← drain rise/fall latches
  [S] output_edges = vtl.output_edges()   ← compute output edges for trigger detection

  scene.advance_animations(input_edges, output_edges, &mut pending_outputs)
    → animations write to stimuli (anim_enabled, user_enabled, position)
    → animations accumulate output bits in pending_outputs
    → completing animations execute final actions

  tessellate / record Vulkan command buffer
  vkQueueSubmit
  vkQueuePresentKHR                             ← frame N queued, visible at vblank N+1

  [C] pending_outputs_prev = take(pending_outputs)   ← save for next [A]
── vblank N+1 fires ────────────────────────────────────────────────────────
```

---

## Verification plan

1. **Unit** (`vtl/tests/`): create `VtlOwner`, write bits from a thread, read edges with `poll()`, confirm latches clear.
2. **Integration** (null renderer): via ZMQ, create a `FlashForNFrames`, arm it, fire `SetVirtualTriggerLine` (kind=INPUT) software trigger, assert stimulus `enabled` changes across `advance_animations`.
3. **Final action test**: animation with `TOGGLE_PHOTODIODE | RESTART` — confirm photodiode toggles each time flash completes and animation re-arms.
4. **Overlay**: VTL panel, use "Fire rising" button, confirm level updates.
5. **Output ordering** (integration): animation A (FlashForNFrames, FINAL_ACTION_TRIGGER_LINE → bit X) + animation B (FlashForNFrames, start_trigger = bit X). Fire A. Assert B does NOT start until frame N+1.
6. **Input immutability**: after `advance_animations`, verify no input shm bits modified.
