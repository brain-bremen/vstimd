# Tutorial: Triggers & animations (VTL)

This is the **frame-accurate, on-device** path. Where the [command API](command-api.md)
runs one round-trip per change, the trigger/animation path uploads a small
declarative behaviour *ahead of time* and lets the render loop execute it in
hardware time — reacting to external signals and emitting markers without any
network round-trip.

Two building blocks:

- **Virtual Trigger Lines (VTL)** — a shared-memory bank of trigger bits. Input
  lines carry signals *into* vstimd (a TTL from a DAQ, an Arduino, a photodiode, or
  a software simulation). Output lines carry signals *out* (stimulus-onset markers,
  frame-sync pulses).
- **Animations** — pre-uploaded behaviours (*flash*, *flicker*, *couple visibility
  to a line*, *move along a path*) with a vocabulary of **start / final / cancel
  actions**. An animation can be *armed* to wait for a trigger edge, run for an
  exact number of frames, and pulse an output line on the way in or out.

!!! info "The frame contract"
    Every frame, at **vblank**, vstimd (1) polls input lines and drains their
    rising/falling-edge latches, (2) advances all animations, then (3) commits
    output lines. So a reaction and its output marker are locked to the exact frame
    the stimulus changes. See [Frame timing](../concepts/frame-timing.md).

## 1. Name your trigger lines

Lines are addressed by `(bank, bit)` and a **kind** (input vs. output — they are
independent signals that share the same index space). Give them names so the rest
of your code — and your DAQ bridge — reads clearly.

```python
from vstimd import Connection, VtlKind

with Connection("tcp://stimulus-pc:5555") as conn:
    # Input line the DAQ will pulse when a trial starts:
    conn.vtl.set_line_name(bank=0, bit=0, kind=VtlKind.INPUT,  name="trial_start")
    # Output line vstimd pulses when the stimulus appears:
    conn.vtl.set_line_name(bank=0, bit=0, kind=VtlKind.OUTPUT, name="stim_onset")

    for line in conn.vtl.list_lines():
        print(line.kind, line.bank, line.bit, line.name, "HIGH" if line.high else "low")
```

Build a handle from either coordinates or a registered name:

```python
from vstimd import VtlHandle, VtlKind

trial_start = VtlHandle.named("trial_start", VtlKind.INPUT)
stim_onset  = VtlHandle.named("stim_onset",  VtlKind.OUTPUT)
# equivalently:
trial_start = VtlHandle.input(bank=0, bit=0)
stim_onset  = VtlHandle.output(bank=0, bit=0)
```

## 2. Flash a stimulus for an exact number of frames

The simplest animation. Create a stimulus (disabled), then a `flash` animation over
it, arm it, and it runs immediately for the given duration:

```python
from vstimd.stimuli import Vec2, Color

target = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0), width=200, height=200,
                                         color=Color(1, 1, 1))
conn.stimuli.set_enabled(target, False)

flash = conn.animations.create_flash(target, duration_frames=10)   # or duration_ms=166
conn.animations.arm(flash)   # armed with no trigger → starts on the next frame
```

The stimulus is shown for exactly 10 frames and then hidden — timed by the render
loop, not by your script. Durations can be given in frames or milliseconds
(converted using the measured refresh rate).

## 3. React to an external trigger

Now make the flash wait for a hardware edge. Pass `start_trigger` so the animation
sits **armed** until the line fires, then runs:

```python
flash = conn.animations.create_flash(
    target,
    duration_frames=10,
    start_trigger=trial_start,          # wait for the DAQ's trial-start TTL
    start_edge=VtlEdge.RISING,
)
conn.animations.arm(flash)
# ...nothing happens until the input line goes high on the device...
```

When the rising edge is detected at the start of a frame, the flash begins **on
that frame** — no round-trip to your experiment PC. This is the core reason to use
the VTL path: the reaction latency is one frame, deterministically, regardless of
what your OS or network is doing.

### Simulating the trigger in software

For testing without hardware, drive the input line over ZMQ. This writes the same
shared-memory latch a DAQ bridge would:

```python
conn.vtl.set_line(trial_start, True)    # rising edge → arms the flash
conn.vtl.set_line(trial_start, False)
```

## 4. Emit a stimulus-onset marker

To timestamp the stimulus in your recording system, have the animation **pulse an
output line** on the frame it starts. Use the start-action mask plus the output
handle:

```python
from vstimd import StartAction

flash = conn.animations.create_flash(
    target,
    duration_frames=10,
    start_trigger=trial_start,
    start_action_mask=StartAction.START_ACTION_TRIGGER_LINE,
    start_action_trigger_line=stim_onset,     # pulse HIGH for one frame at start
)
conn.animations.arm(flash)
```

Now the sequence *input edge → stimulus onset → output marker* happens entirely on
the device, all locked to the same frame. Wire `stim_onset` into your ephys/imaging
acquisition and the event is timestamped in your neural-data clock — see
[Integrating recording systems](recording-integration.md).

The action masks let you attach behaviour to each phase of the animation:

| Phase | Mask | Useful bits |
|---|---|---|
| Armed → Running (start) | `StartAction` | `ENABLE`, `TOGGLE_PHOTODIODE`, `START_ACTION_TRIGGER_LINE` |
| Completion (final) | `FinalAction` | `DISABLE`, `TOGGLE_PHOTODIODE`, `FINAL_ACTION_TRIGGER_LINE`, `RESTART`, `RESTORE_STATE`, `END_DEFERRED` |
| Cancellation | `CancelAction` | `DISABLE`, `TOGGLE_PHOTODIODE`, `CANCEL_ACTION_TRIGGER_LINE`, `RESTORE_STATE`, `END_DEFERRED` |

Masks are `IntFlag`s, so combine them with `|`:

```python
from vstimd import FinalAction
final = FinalAction.DISABLE | FinalAction.FINAL_ACTION_TRIGGER_LINE
```

## 5. Other animation bodies

Beyond `flash`, the animation vocabulary covers the common timing-critical
behaviours:

```python
# Couple visibility directly to a line's level (on while HIGH):
conn.animations.create_couple_visibility_to_trigger_line(trial_start, target, polarity=True)

# Set enabled once, on a specific edge:
conn.animations.create_enable_on_trigger_edge(trial_start, target,
                                              edge=VtlEdge.RISING, enabled=True)

# Flicker on/off (e.g. 2 frames on, 2 off), optionally for a fixed total:
conn.animations.create_flicker(target, on_frames=2, off_frames=2, total_frames=120)

# Move along a per-frame path, or along waypoints at a constant speed:
conn.animations.create_move_along_path_2d(target, x=[...], y=[...])
conn.animations.create_move_along_segments_2d(target, x=[0, 300], y=[0, 0],
                                              speed_px_per_sec=400)

# Read position from a shared-memory float array each frame (closed-loop):
conn.animations.create_external_position_2d(target, shm_name="/vstimd_pos_myobj")
```

All of them accept the same `start_trigger` / `*_action_mask` / `cancel_trigger`
options as `flash`, so any of them can be armed on a hardware edge and can emit
markers.

## 6. Chaining reactions on the device

Because vstimd both **reads input lines and writes output lines every frame**, an
animation can be armed on an **output** edge — i.e. one animation's marker becomes
another animation's trigger, entirely inside the server:

```python
# Animation A pulses "stim_onset" when it starts.
# Animation B watches that same output line and reacts one frame later —
# no client involved.
second = conn.animations.create_flash(
    other_stim,
    duration_frames=5,
    start_trigger=VtlHandle.named("stim_onset", VtlKind.OUTPUT),
    start_edge=VtlEdge.RISING,
)
conn.animations.arm(second)
```

This lets you build multi-step, frame-locked stimulus sequences that run
autonomously on the device, synchronised to the display and to DAQ markers.

## 7. Managing animation lifecycle

```python
conn.animations.arm(handle)        # IDLE → ARMED  (waiting, or running if no trigger)
conn.animations.disarm(handle)     # back to IDLE, no actions applied
conn.animations.cancel(handle)     # terminal: applies cancel_action_mask, then DONE
conn.animations.delete(handle)     # remove entirely

for a in conn.animations.list_animations():
    print(a.handle, a.name, a.state, a.type_name)

details = conn.animations.query(handle)   # full config + current state
```

An animation with `FinalAction.RESTART` re-arms itself on completion — handy for a
free-running flicker or a repeating cue.

## Inspecting VTL state from the shell

The shared-memory segment lives at `/dev/shm/vstimd_vtl` and can be read directly
for debugging without any client. The `vtl` crate's
[README](https://github.com/braemons/vstimd/blob/main/vtl/README.md) documents the
byte layout and includes ready-made `xxd` / Python snippets to dump line names and
live state.

## Next

- **[Integrating recording systems](recording-integration.md)** — wiring VTL output
  markers into ephys/imaging clocks, and TTL inputs from DAQs and Arduinos.
- **[Frame timing](../concepts/frame-timing.md)** — the guarantee that makes
  on-device reactions trustworthy.
