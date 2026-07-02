# Tutorial: Integrating recording systems

The reason to put stimulus rendering on a dedicated device with real trigger lines
is that electrophysiology and imaging rigs already run on **TTL logic**. They
record incoming TTLs alongside neural data, and emit TTLs to drive cameras, lasers,
and reward hardware. vstimd's [Virtual Trigger Lines](vtl-and-animations.md) plug
straight into that world.

This page covers the two directions of integration and how the timing lines up.

## The shape of the integration

```
   Recording system (ephys / imaging)                 Stimulus device (vstimd)
 ┌───────────────────────────────────┐               ┌──────────────────────────┐
 │ digital acquisition               │  TTL: onset   │ VTL output "stim_onset"  │
 │  ─ records TTLs w/ neural data ◄──┼───────────────┤   pulsed at vblank on the │
 │                                   │               │   exact onset frame       │
 │  ─ emits TTLs (trial gate, …) ────┼───────────────► VTL input "trial_start"  │
 └───────────────────────────────────┘  TTL: trigger └──────────────────────────┘
                    ▲                                          ▲
                    │  the SAME acquisition clock timestamps   │
                    └──────────  both sides  ──────────────────┘
```

The key property: because the onset marker is recorded **by your acquisition
system, in its own clock**, you never have to align two software timelines after
the fact. The stimulus event and the neural data share one timebase.

## The hardware bridge

Real TTL lines reach vstimd through a small companion daemon that maps physical
pins onto the VTL shared-memory bank. On a Jetson/Pi that is `gpiochip-daqd`
(GPIO pins); other backends (NI-DAQ, LabJack, …) follow the same pattern. The
daemon:

- **writes input lines** — latches a rising/falling edge into the VTL bank when a
  pin changes, which vstimd drains at the next vblank;
- **reads output lines** — is woken once per frame by an output strobe, then pulses
  the corresponding physical pins.

An **Arduino or microcontroller** works just as well as a lab DAQ for either
direction: pulse a GPIO into the bridge to trigger a stimulus, or read vstimd's
output pin to gate other equipment. From vstimd's side it is all just trigger
lines — see the [`vtl` crate README](https://github.com/braemons/vstimd/blob/main/vtl/README.md)
for the shared-memory contract a custom bridge would implement.

!!! note "You don't write DAQ code inside vstimd"
    vstimd never talks to DAQ hardware directly. It only reads and writes the
    shared-memory VTL bank; the bridge daemon owns the hardware. That keeps the
    render loop free of driver code and lets you swap DAQ backends without touching
    the stimulus server.

## Direction 1 — Markers out (timestamp your stimuli)

Have the animation that shows a stimulus pulse an **output line** on its onset
frame. Wire that output pin into a spare digital input on your acquisition system.

```python
from vstimd import Connection, VtlHandle, VtlKind, VtlEdge, StartAction, FinalAction

with Connection("tcp://stimulus-pc:5555") as conn:
    conn.vtl.set_line_name(bank=0, bit=0, kind=VtlKind.OUTPUT, name="stim_onset")
    stim_onset = VtlHandle.named("stim_onset", VtlKind.OUTPUT)

    target = conn.stimuli.shapes.create_circle(pos=(0, 0), radius=100)
    conn.stimuli.set_enabled(target, False)

    flash = conn.animations.create_flash(
        target,
        duration_frames=30,
        start_action_mask=StartAction.ENABLE | StartAction.START_ACTION_TRIGGER_LINE,
        start_action_trigger_line=stim_onset,      # HIGH for one frame at onset
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(flash)
```

Every acquisition system that records a TTL will now have your stimulus onset in
its own timeline, frame-accurate. Add a **photodiode** on the panel (toggle it with
`StartAction.TOGGLE_PHOTODIODE`) if you want an optical ground-truth of the actual
light change to cross-check the electronic marker.

## Direction 2 — Triggers in (let the rig drive stimuli)

Point the other way: your recording system (or a behavioural controller, lever, or
Arduino) emits a TTL that vstimd reacts to. Arm the animation on a **VTL input
line**.

```python
    conn.vtl.set_line_name(bank=0, bit=1, kind=VtlKind.INPUT, name="trial_gate")
    trial_gate = VtlHandle.named("trial_gate", VtlKind.INPUT)

    flash = conn.animations.create_flash(
        target,
        duration_frames=30,
        start_trigger=trial_gate,          # wait for the rig's TTL
        start_edge=VtlEdge.RISING,
        start_action_mask=StartAction.ENABLE | StartAction.START_ACTION_TRIGGER_LINE,
        start_action_trigger_line=stim_onset,
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(flash)
```

The stimulus now appears one frame after the external edge, deterministically, and
*also* emits its own onset marker — so the trigger-to-stimulus and
stimulus-to-marker delays are both fixed and known.

## Closed-loop position from an external source

For closed-loop work (e.g. moving a stimulus with an eye/hand tracker or a physics
model running elsewhere), a producer writes 2-D positions into a shared-memory
float array and vstimd reads it **every frame**:

```python
    conn.animations.create_external_position_2d(target, shm_name="/vstimd_pos_cursor")
```

The position update is applied on the render loop, so it is as timely as the
display allows — no per-sample network round-trip.

## Why this beats aligning logs after the fact

Software-only toolkits typically record stimulus events in the experiment PC's
clock and rely on post-hoc synchronisation (or an extra photodiode channel) to line
them up with neural data. With vstimd the timing-critical path — *trigger →
stimulus change → recorded marker* — is realised in **hardware, on the render
loop**, and the marker is captured by the acquisition system itself. There is no
second clock to reconcile.

## See also

- **[Triggers & animations](vtl-and-animations.md)** — the full VTL and animation API.
- **[Frame timing](../concepts/frame-timing.md)** — how onset frames are pinned to
  vblank.
- **[`vtl` crate README](https://github.com/braemons/vstimd/blob/main/vtl/README.md)** —
  shared-memory layout for building a custom DAQ bridge.
