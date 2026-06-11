# vstimd for PsychoPy Users

## Overview

The native vstimd Python API is command-oriented and maps closely to the
underlying protobuf/ZeroMQ protocol: you create stimuli, set properties, and
send commands that are executed by the server.  The `vstimd.psychopy` layer
wraps this command API in object-oriented classes that mirror the
`psychopy.visual` interface.

```{important}
As of v0.1, the PsychoPy-compatible layer does **not** expose vstimd's
**animation** or **VTL** (Virtual Trigger Lines) systems.  Those systems
are the primary mechanism for vstimd's sub-millisecond frame-timing
guarantees.  Use the native command API directly when precise stimulus
timing matters.
```

## Quick start — drop-in import swap

```python
# Before
from psychopy import visual

# After
from vstimd.psychopy import visual
```

Most experiment code works unchanged after this substitution.

---

## Connecting to the server

The connection address is set on the `Window` object via the `address`
parameter.  This is the **only place** you specify the server's IP and port.

```python
# Same machine (default — tcp://localhost:5555)
win = visual.Window()

# Remote machine
win = visual.Window(address='tcp://192.168.1.10:5555')
```

The ZMQ endpoint format is `tcp://<host>:<port>`.

---

## Migration reference

| psychopy | vstimd | Notes |
|---|---|---|
| `from psychopy import visual` | `from vstimd.psychopy import visual` | direct swap |
| `Window(size=...)` | `Window(address='tcp://host:port')` | `size` is ignored — queried from server |
| `Circle(win, ...)` | identical | ✓ |
| `Rect(win, ...)` | identical | ✓ |
| `GratingStim(win, ...)` | identical | ✓ |
| `Polygon(win, ...)` | not in v0.1 | raises `AttributeError` |
| `Line(win, ...)` | not in v0.1 | raises `AttributeError` |
| `ShapeStim(win, ...)` | not in v0.1 | raises `AttributeError` |
| `TextStim` | not in v0.1 | raises `AttributeError` |
| `TextBox2(win, ...)` | identical | ✓ text, color, pos, opacity, autoDraw, languageStyle |
| `ImageStim` | not in v0.1 | raises `AttributeError` |
| `win.flip()` | identical | sends batch to server |
| `stim.draw()` | identical | one-shot per frame |
| `stim.autoDraw = True` | identical | always rendered |
| `contains()` / `overlaps()` | not in v0.1 | raises `NotImplementedError` |

### No-op stubs

- `monitor=` on Window is accepted for future deg/cm units but ignored if units are `pix`/`norm`/`height`
- `autoLog=` is accepted but logging is not wired up yet
- `contrast=` is accepted but not forwarded to the server yet

---

## Deferred vs immediate mode

### `deferred=True` (default — matches PsychoPy frame model)

Property setters send commands to the server's deferred queue immediately.
`win.flip()` tells the server to apply the entire queue atomically before the
next render frame.

```python
win = visual.Window(deferred=True)   # default
circle = visual.Circle(win, radius=50)
circle.pos = (100, 0)   # queued on server
circle.opacity = 0.8    # queued on server
win.flip()              # ← server applies all queued commands before next vsync
```

### `deferred=False` (immediate)

Every property setter sends a ZMQ command immediately.  `win.flip()` is a
no-op.  Use this for interactive / exploratory use, not for time-critical
experiments.

```python
win = visual.Window(deferred=False)
circle.pos = (100, 0)   # sent immediately
```

---

## Unit system

All coordinates are converted to pixels before being sent to the server.
The server's origin is the window centre (matches PsychoPy default).

Supported units: `pix` (default), `norm`, `height`.

`deg` and `cm` require a PsychoPy `Monitor` object:

```python
from psychopy.monitors import Monitor
mon = Monitor('testMonitor', width=52.0, distance=57.0)
win = visual.Window(monitor=mon, units='deg')
circle = visual.Circle(win, radius=2.0, units='deg')
```

---

## Asset transfer (v0.1)

In v0.1, the only supported mechanism is **path reference**: pass an absolute
filesystem path as a string.  The server loads the asset from disk.

Inline binary (numpy arrays, PIL Images) and chunked upload for remote/large
assets are planned for v0.2.

---

## Running tests

```bash
cd client/python

# Unit tests (no server required)
make test

# E2E against the null renderer (builds server binary automatically)
make test-e2e-null

# E2E against a real running server
VSTIM_SERVER_ADDR=tcp://192.168.1.10:5555 make test-e2e
```
