# Why vstimd?

[**PsychoPy**](https://www.psychopy.org/),
[**Psychtoolbox**](http://psychtoolbox.org/), and
[**MonkeyLogic**](https://monkeylogic.nimh.nih.gov/) are excellent, mature,
widely-used tools, and for most experiments they are exactly the right choice.
They are also broad **experiment-control frameworks**: they handle trial logic,
input devices, reward, data logging, *and* stimulus drawing — and they do it all
on the same computer that draws the screen, typically a general-purpose workstation
running a full desktop OS. vstimd is not trying to replace them; it targets a
narrow slice they necessarily trade off against everything else they do.

vstimd takes a different shape, and a deliberately **narrower scope**. It splits the
experiment into two halves:

- a **dedicated stimulus device** whose only job is to render frames on time, and
- your **experiment logic**, which runs wherever is convenient and talks to the
  device over the network.

!!! note "A smaller scope, on purpose"
    vstimd is **only** a visual-stimulation server — it does not run your trial
    logic, manage reward, or log behaviour. Anything beyond drawing frames —
    behavioural logic, eye tracking, reward control — is handled by separate
    systems, which may run on the same machine as vstimd or on their own. That is a
    feature, not a gap: by doing one thing, vstimd can do it *well* — guaranteed
    frame timing on cheap hardware. Its modular, network-first architecture
    (protobuf/ZMQ commands + shared-memory trigger lines) means it drops into
    whatever experiment-control stack you already use — including PsychoPy, via a
    [compatibility layer](tutorial/command-api.md#psychopy-compatibility) — rather
    than replacing it.

## Related work and lineage

vstimd's shape is not new; it deliberately borrows from prior systems.

- **Client/server rendering.** Running the renderer as a network-driven server,
  separate from the controlling client, comes from two lineages: Michael Stephan's
  [**StimServer**](https://github.com/esi-neuroscience/StimServer) (ESI
  Neuroscience), a **Windows-only** C++ stimulus server, and **MWorks**, whose
  MWServer draws stimuli while MWClient controls it — but MWorks is **macOS-only**,
  server *and* client.
- **Trigger-driven logic.** Reacting to external events frame-by-frame is inherited
  from StimServer — which used **named events** *and* **shared memory** — and
  echoes Andreas Kreiter's **VStim**. vstimd generalises this into its
  [Virtual Trigger Lines](tutorial/vtl-and-animations.md): a named, shared-memory
  bank of trigger bits polled every frame.

What vstimd adds is the *substrate*. Where StimServer renders through the Windows
desktop and MWorks through the macOS window server — both on general-purpose
workstations — vstimd reimplements the whole stack in Rust on **bare-metal Linux**:
a dedicated embedded board rendering directly on the KMS/DRM plane with direct
vblank control, no compositor and no desktop OS in the timing path, speaking to
clients in *any* language on *any* OS.

This page explains what that focus buys you, and when it complements — rather than
competes with — the frameworks above.

!!! tip "Best of both: use them together"
    A great setup often pairs the two. Keep your trial logic, reward, and data
    logging in PsychoPy or MonkeyLogic, where they shine, and hand the
    *timing-critical drawing* to a vstimd device over the network — PsychoPy code
    can even talk to it through the
    [PsychoPy-compatible layer](tutorial/command-api.md#psychopy-compatibility).
    vstimd is most valuable when timing must be *guaranteed*, when the rig is
    *embedded or compact*, or when stimulus timing has to be *fused with
    electrophysiology or imaging*.

## 1. A dedicated device with guaranteed frame timing

On a general-purpose OS, your stimulus script shares the CPU with the compositor,
background daemons, and the scheduler. Most frames are fine; the occasional one is
late — and in a timing-sensitive experiment a single dropped frame is a
contaminated trial.

vstimd runs on a small, **low-cost embedded board** (NVIDIA Jetson Orin Nano,
Raspberry Pi 5, or similar) and renders **directly on the Linux KMS/DRM display
plane — with no X11, no Wayland, and no compositor**. The render loop owns the
display and blocks on the true hardware vblank, so:

- frame flips are locked to the panel's refresh, not to OS scheduling luck;
- there is no compositor between your frame and the glass adding a variable frame
  of latency;
- the box does *one thing*, so nothing else can steal a time slice at the wrong
  moment.

The result is **frame-accurate, low-jitter timing that you can measure and
certify** (see [Frame timing](concepts/frame-timing.md)), on hardware that costs a
fraction of a lab workstation.

## 2. Home-cage training and compact setups

Because the renderer is a self-contained board rather than a tower PC, the whole
stimulus half of the rig shrinks to something you can mount **inside a home cage,
a behaviour box, or a portable setup**.

- **Home-cage training.** Put the device and a small panel on the cage. It boots
  straight into the stimulus server (`vstimd.target`, see
  [Deployment](deploy.md)), runs unattended, and needs no monitor, keyboard, or
  desktop session. Animals can self-train around the clock while the controlling
  logic runs elsewhere — or entirely on the device.
- **Many rigs, one design.** Cheap identical boards make it affordable to build a
  bank of matched training stations instead of one shared expensive rig.
- **Field / mobile work.** Low power draw and no display-server dependency make
  the same setup viable outside the lab.

## 3. Control it from (almost) anything

The device is driven over the network with **protobuf messages over ZeroMQ**. The
wire protocol is language- and platform-neutral, so the controlling client can be:

- **Python / PsychoPy** on Linux, Windows, or macOS,
- **MATLAB**,
- **C# / Bonsai**,
- or anything that can open a TCP socket and encode protobuf — including a
  microcontroller or another embedded board.

Your analysis stack, your behavioural task engine, and your stimulus renderer no
longer have to live in the same language or on the same machine. See
[Choosing an API path](tutorial/index.md).

## 4. Built to plug into ephys and imaging

This is where a dedicated device with real trigger lines really pays off.

Electrophysiology and imaging systems are built around **TTL digital lines**: they
record incoming TTLs alongside neural data, and they emit TTLs to trigger cameras,
lasers, and reward pumps. vstimd works in the same currency through
**Virtual Trigger Lines (VTL)** — a shared-memory bank of trigger bits that a small
companion daemon (`daqd`) bridges to real hardware lines. Like any stimulus system,
vstimd needs a DAQ for physical TTLs — but on a Raspberry Pi or NVIDIA Jetson the
**onboard GPIO** already serves that role (`gpiochip-daqd`), so no add-on board is
required; an external DAQ or an Arduino works just as well:

- **Stimulus-onset markers out.** When a stimulus appears, vstimd pulses an
  **output line at the exact frame it becomes visible**. Wire that TTL into your
  ephys/imaging acquisition and every event is timestamped in the *same clock as
  your neural data* — no post-hoc alignment guesswork.
- **Triggers in.** A TTL from your recording system, a photodiode, a lever, or a
  microcontroller can **drive stimuli directly on the device**, frame-accurately,
  with no round-trip back to the experiment PC.
- **Reactions stay on the box.** Because vstimd both reads input lines and writes
  output lines every frame, stimulus reactions to triggers happen in *hardware
  time* on the render loop — not at the mercy of network and OS latency.

That means the timing-critical path — *trigger → stimulus change → recorded
marker* — never leaves the real-time device, while your high-level task logic
stays in whatever language and machine you prefer.

See [Triggers & animations](tutorial/vtl-and-animations.md) and
[Integrating recording systems](tutorial/recording-integration.md).

## At a glance

This is not a scorecard — every tool below is mature and capable, and most of them
cover far more ground than vstimd does. The matrix simply lays out the trade-offs
vstimd makes in exchange for its narrow focus on frame timing. Cells are necessarily
simplified.

<div class="compare-matrix" markdown>

| | PsychoPy | Psychtoolbox | MonkeyLogic | MWorks | StimServer | vstimd |
|---|---|---|---|---|---|---|
| **Scope** | Full control | Rendering + I/O | Full control | Full control | Visual only | Visual only |
| **Architecture** | Single-box | Single-box | Single-box | Client/server | Client/server | Client/server |
| **Renders on** | PC (any OS) | PC (any OS) | PC (Windows) | Mac | PC (Windows) | Embedded Linux |
| **Display path** | Compositor | Compositor | Compositor | macOS window server | Windows desktop | Direct KMS/DRM |
| **Frame timing** | Best-effort (shares OS) | Best-effort + timing tools | Best-effort (shares OS) | Best-effort (shares OS) | Best-effort (shares OS) | Vblank-locked, dedicated core |
| **Server-side animations** | Per-frame in your loop | Per-frame in your loop | Task-loop | State-system | Client-driven | On-device, trigger-armed |
| **Client / platform** | None; any OS | None; any OS | None; Windows | **macOS only** | Windows | **Any lang / OS, networked** |
| **Digital I/O (TTL)** | Add-on HW | Add-on HW | NI-DAQ | I/O plugins + HW | Host DAQ | `daqd`; onboard GPIO on Pi / Jetson |
| **External triggers** | Polled in loop | Polled in loop | DAQ → state machine | I/O → state system | Named events / shm | TTL → VTL, per-frame, on-device |
| **Hardware** | Workstation | Workstation | Workstation + DAQ | Mac | Workstation | Low-cost SBC |
| **Home-cage rigs** | Heavier | Heavier | Lab rigs | Possible (Mac) | Possible (Win) | First-class |

</div>

## Where to go next

- **[Choosing an API path](tutorial/index.md)** — the two ways to drive vstimd and
  when to use each.
- **[Quick start](getting-started/quick-start.md)** — send your first stimulus in
  a few lines.
- **[Frame timing](concepts/frame-timing.md)** — how the guarantee is achieved and
  measured.
