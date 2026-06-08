# TODO

## VTL (Virtual Trigger Lines) — design resolved, implementation in progress

The design questions below are now answered.  Open implementation work is tracked
in `dev/vtl-plan.md` (step-by-step) and flagged with `// TODO: VTL` in the code.

---

### Resolved design decisions

**1. Input / output split**

The split mirrors the real DAQ hardware model.  daqd acts as a facade: lines that
are inputs to daqd are also inputs to vstimd; lines that are outputs from vstimd
are read by daqd as outputs.

| Direction | Canonical writer | Canonical reader |
|---|---|---|
| Input  | nidaqd (hardware edge) or ZMQ `SetInput*` (software/test) | vstimd render loop (`VtlState::poll`) |
| Output | vstimd render loop (animations + vblank trigger)          | nidaqd (drives hardware DAQ lines) |

**vstimd never writes input lines** — not in the render loop, not in animations.
ZMQ `SetInput*` commands exist to simulate hardware trigger inputs for testing.

ZMQ `SetOutput*` commands exist for manual override and debugging only.  In normal
operation output lines are driven exclusively by the render loop.

**2. Animation → trigger integration**

Animations may use **both input and output lines** as start triggers.  An output
line set by animation A in frame N is visible to animation B as a trigger at the
start of frame N+1 (1-frame latency).  This allows one animation to chain into
another via output lines without any Python state-machine involvement.

Rule: **animation output changes are accumulated during the animation pass and
committed to shared memory only after all animations have been updated for that
frame.**  This prevents an animation that fires an output bit from triggering
another animation in the same frame (ordering-effect prevention).

**3. Frame / vblank trigger**

A designated output line (the "vblank trigger") represents the frame period — it
goes high at vblank and is driven by a fast path in the render loop **before** the
animation pass, not via the deferred animation output commit.  This ensures it is
always written with minimum latency regardless of animation ordering.

**4. Exact per-frame timing**

`VtlState::poll()` and `VtlState::write_outputs()` are implemented but not yet
wired into the render loop.  See `// TODO: VTL` placeholders in
`server/src/render/drm/mod.rs` and `server/src/render/winit_vk/mod.rs`.

The chosen frame timeline (Option B with output-snapshot for animation triggers):

```
── vblank N fires ──────────────────────────────────────────────────────────────
  (DRM: wait_vblank() returns — hardware scan-out flip just occurred)

  [A] OUTPUT COMMIT + VBLANK TRIGGER HIGH
        write_outputs(output_pending_prev | vblank_mask)
        Animation outputs from the previous frame are committed here, aligned
        with the scan-out flip.  The vblank trigger bit is ORed in separately:
        it goes HIGH now to signal that vstimd is actively computing this frame.

  [A] INPUT POLL — VtlState::poll()
        Drains input rise/fall latches; returns VtlEdges (rising, falling, current).

  [S] OUTPUT SNAPSHOT — read current output_state from shm.
        Frozen copy for animation trigger detection.  Includes bits just committed
        at [A] (animation outputs + vblank HIGH).  Using a snapshot prevents
        same-frame ordering effects.

  animations run:
    read VtlEdges (input edges) + output snapshot (output-line levels/edges)
    update stimuli
    accumulate output changes in output_pending[] for the NEXT frame's [A]
    (completing animations execute final actions: DISABLE, SIGNAL_EVENT, etc.)

  tessellate / record / submit / present

  [C] VBLANK TRIGGER LOW
        write_outputs(output_pending_prev)   ← same as [A] but without vblank_mask
        Clears the vblank trigger bit.  Animation outputs remain unchanged.
        The HIGH→LOW transition marks when frame rendering was submitted.
        nidaqd sees a pulse whose width = time from vblank to present submit.

  save output_pending for next iteration

── vblank N+1 fires ────────────────────────────────────────────────────────────
  frame N becomes visible on the display
  [A] commits output_pending from frame N (animation outputs aligned with scan-out)
      and raises vblank trigger again for frame N+1
```

---

### Design options (archived)

The three options below were considered during design.  **Option B** was chosen
as the basis, extended with the output-snapshot rule for animation chaining.

#### Option A — Unified triggers, direction is annotation only

One trigger bank.  No input/output split in the vstimd API.  `direction` on
a named line is a human-readable label only, not enforced.

Pros: simplest API, fewest concepts.
Cons: nothing stops a misconfigured client from writing to a "hardware input"
bit, potentially fighting nidaqd; no machine-readable way to know which bits
drive hardware.

---

#### Option B — Split banks, animations write outputs only (CHOSEN)

Two physical banks: `input_state` (nidaqd writes) and `output_state`
(animations write).  ZMQ split commands retained.  Animations read inputs,
write outputs.  Extended: animations may also read the previous frame's output
snapshot to detect output-line edges (for animation-to-animation chaining).

Pros: clear ownership, maps to real hardware model, no fighting over bits.
Cons: more API surface; Python state machine path adds 1–2 frames latency
(~17–33 ms at 60 Hz); nidaqd path is fast enough (<1 ms).

---

#### Option C — Unified triggers, snapshot/commit, direction annotation

Like A but with explicit snapshot and commit steps.  Not chosen — same ownership
ambiguity as A.
