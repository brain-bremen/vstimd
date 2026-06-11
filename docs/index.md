# vstimd

**vstimd** is a visual stimulus server for neuroscience experiments. It runs on dedicated
hardware and accepts commands from experiment scripts over the network, rendering stimuli with
precise, vsync-locked frame timing.

```
Experiment PC                    Stimulus PC (Linux, DRM)
┌─────────────┐    ZMQ/TCP      ┌──────────────────────┐
│ Python /    │ ──────────────► │  vstimd              │
│ MATLAB /    │                 │  (Vulkan + KMS/DRM)  │ ──► Monitor
│ C#  script  │ ◄────────────── │                      │
└─────────────┘    protobuf     └──────────────────────┘
```

## Key features

- **Frame-accurate stimulus timing** — vsync-locked render loop, DRM vblank wait
- **Cross-language clients** — Python, MATLAB, C# (and PsychoPy-compatible Python layer)
- **Bare-metal Linux rendering** — runs without a compositor (X11/Wayland) via KMS/DRM
- **Deferred mode** — batch multiple stimulus changes into a single atomic frame flip
- **Live debug overlay** — frame timing, stimulus list, command log (toggle with F1)

## Stimulus types

| Type | Description |
|---|---|
| Rectangle | Axis-aligned filled rectangle with optional outline |
| Circle | Filled circle |
| Ellipse | Filled ellipse |
| Grating | Analytical sinusoidal grating with aperture masks and drift |
| Text | Rendered text with configurable font, size, colour, and anchor |

## Quick start

=== "Python"

    ```python
    from vstimd import Connection
    from vstimd.stimuli import Vec2, Color

    with Connection("tcp://stimulus-pc:5555") as conn:
        h = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0), width=200, height=100,
                                            color=Color(1.0, 0.0, 0.0))
        conn.stimuli.set_enabled(h, True)
        conn.stimuli.delete(h)
    ```

=== "PsychoPy"

    ```python
    from vstimd.psychopy import visual

    win = visual.Window(address="tcp://stimulus-pc:5555")
    rect = visual.Rect(win, width=0.5, height=0.25, fillColor="red")
    rect.draw()
    win.flip()
    ```

=== "MATLAB"

    ```matlab
    conn = vstimd.Connection('tcp://stimulus-pc:5555');
    h = conn.stimuli.create_rect('x', 0, 'y', 0, 'width', 200, 'height', 100, ...
                                 'r', 1.0, 'g', 0.0, 'b', 0.0);
    conn.stimuli.set_enabled(h, true);
    conn.stimuli.delete(h);
    conn.close();
    ```

## Project layout

```
vstimd/
├── server/          Rust server (vstimd binary)
├── client/python/   Python client (vstimd package)
├── client/matlab/   MATLAB client
├── proto/           Protobuf schema (source of truth for all clients)
├── tools/           Timing test tool and utilities
└── docs/            This documentation
```
