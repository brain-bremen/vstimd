# TODO

## VTL (Virtual Trigger Lines) — design not yet finalised

The VTL implementation is work-in-progress.  The following questions must be
answered before the feature is considered stable.

---

### Open questions

**1. Input vs output distinction**

Real DAQ hardware has fixed-direction channels.  nidaqd handles the physical
mapping.  It is undecided how much vstimd should model:

- Should the ZMQ API split commands (`SetInputVirtualTriggerLine` /
  `SetOutputVirtualTriggerLine`) or expose a single `SetVirtualTriggerLine`
  and leave directionality to nidaqd configuration?
- Is `direction` on a named line a hard constraint or a user annotation?
- Can animations write to input lines, or only to output lines?

**2. Animation → trigger integration**

Animations have no VTL access today.  Needed:

- How does a completing animation fire an output trigger?
- Do animations receive a pre-computed edge summary (rising/falling masks)
  or read trigger state directly?
- How are trigger-driven animation transitions expressed?

**3. Exact per-frame timing**

`VtlState::poll()` and `VtlState::write_outputs()` exist but are not wired
into either render loop.  See `// TODO: VTL` placeholders in
`server/src/render/drm/mod.rs` and `server/src/render/winit_vk/mod.rs`.

---

### Design options

#### Option A — Unified triggers, direction is annotation only

One trigger bank.  No input/output split in the vstimd API.  `direction` on
a named line is a human-readable label only, not enforced.

```
frame timeline
──────────────
vblank N
  snapshot: freeze trigger_state → animation system sees this
  animations run:
    read frozen snapshot for edge decisions
    queue any trigger writes to pending[]
  tessellate / record / submit / present
  commit: trigger_state ← pending[]   ← nidaqd reads from here
vblank N+1 — frame N visible, committed triggers live
```

Writers to trigger_state:
- nidaqd (hardware edges, any time — seen at next snapshot)
- ZMQ `SetVirtualTriggerLine` (software trigger / testing)
- Animation commit (via pending[], at frame end)

Pros: simplest API, fewest concepts.
Cons: nothing stops a misconfigured client from writing to a "hardware input"
bit, potentially fighting nidaqd; no machine-readable way to know which bits
drive hardware.

---

#### Option B — Split banks, animations write outputs only

Two physical banks: `input_state` (nidaqd writes) and `output_state`
(animations write).  ZMQ split commands retained.  Animations read inputs,
write outputs.  External state machine bridges: reads outputs, sets inputs.

```
frame timeline
──────────────
vblank N
  [A] INPUT SNAPSHOT: freeze input_state → animation edges visible
  animations run:
    read frozen input snapshot
    update stimuli
    completing animations set bits in output_pending[]
  tessellate / record / submit / present
  [C] OUTPUT COMMIT: output_state ← output_pending[]   ← nidaqd reads
vblank N+1 — frame N visible, output triggers live

external state machine loop (Python, ~1–2 frame latency):
  read output triggers (ZMQ list_lines or dedicated query)
  decide → set input triggers (ZMQ SetInputVirtualTriggerLine)
  → seen at next snapshot

nidaqd fast path (shm direct, sub-ms):
  poll output_state → pulse hardware output channel
  hardware input edge → write input_state → seen at next snapshot
```

Pros: clear ownership, maps to real hardware model, no fighting over bits.
Cons: more API surface; Python state machine path adds 1–2 frames latency
(~17–33 ms at 60 Hz); nidaqd path is fast enough (<1 ms).

Variant B2 — preparation-gated pulse:
Write `output_state` HIGH at [B] (frame start, after snapshot) and LOW at
[C] (frame end, after present).  Trigger is high only during vstimd's compute
time for that frame.  Requires two commit calls per frame.

```
vblank N
  [A] INPUT SNAPSHOT
  [B] OUTPUT WRITE: output_state ← frame_start_outputs  (pulse goes HIGH)
  animations / tessellate / record / submit / present
  [C] OUTPUT WRITE: output_state ← frame_end_outputs    (pulse goes LOW)
vblank N+1
```

---

#### Option C — Unified triggers, snapshot/commit, direction annotation

Like A but with explicit snapshot and commit steps retained from B.
Single trigger bank in the API.  Snapshot at frame start; pending writes
from animations committed at frame end.  `direction` on named lines is
annotation only.  nidaqd uses its own configuration to decide which bits
it reads vs writes.

```
frame timeline
──────────────
vblank N
  snapshot: trigger_snapshot ← trigger_state
  animations:
    read trigger_snapshot (edge detection)
    write pending[]
  tessellate / record / submit / present
  commit: trigger_state ← trigger_state MERGE pending[]
vblank N+1
```

Pros: consistent snapshot semantics, simpler than B, still frame-accurate.
Cons: no enforcement; nidaqd and animations could write the same bit.

---

### Recommendation (tentative)

**Option B** maps most cleanly to the real hardware model and makes
ownership explicit.  The Python state machine latency (1–2 frames) is
acceptable for experiment logic decisions.  The nidaqd shm path is fast
enough for hardware-critical timing.
