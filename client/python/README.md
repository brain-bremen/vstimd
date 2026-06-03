# vstimd Python client

Python client for the `vstimd` visual stimulus server. Talks to the server
over ZMQ using protobuf encoding.

## Install

```bash
cd client/python
uv sync
```

## Quick start

```python
from vstimd import Connection

with Connection() as conn:
    h = conn.stimuli.create_rect(x=-200, y=0, width=300, height=200,
                                 r=1.0, g=0.0, b=0.0)
    conn.stimuli.set_enabled(h, False)
    conn.stimuli.delete(h)
    info = conn.system.query_server_info()
    print(info.version)
```

`Connection(address="tcp://localhost:5555")` — default address shown.

## Package layout

```
vstimd/
  __init__.py          # exports Connection, ServerInfo, exceptions, psychopy
  _connection.py       # ZMQ REQ socket + protobuf send/recv
  _proto/              # generated stubs (common, service, stimuli_2d, system)
  stimuli/             # StimuliClient — create/mutate/query stimulus objects
  system/              # SystemClient  — scene-wide commands and server queries
  psychopy/            # PsychoPy-compatible visual layer (see below)
  exceptions.py        # error code → Python exception mapping
examples/
  flash_rects.py       # create two rects, flash them alternately
  interactive_grating.py
tests/
  unit/                # API signature tests (no server required)
  e2e/                 # full round-trip tests against the null renderer or a real server
```

## `conn.stimuli` — StimuliClient

### Creation (all return an integer handle)

| Method | Creates |
|---|---|
| `create_rect(*, x, y, width, height, r, g, b, a, name, id)` | Rectangle |
| `create_circle(*, x, y, radius, r, g, b, a, name, id)` | Circle |
| `create_ellipse(*, x, y, width, height, angle, r, g, b, a, name, id)` | Ellipse |
| `create_grating(*, pos, width, height, sf, phase, angle, contrast, fore_color, back_color, opacity, waveform, mask, mask_param, drift_speed, drift_decoupled, drift_angle, name, id)` | Grating |
| `create_text(*, x, y, width, height, text, font_family, letter_height_px, r, g, b, a, anchor, language_style, name, id)` | Text |

All `create_*` methods accept an optional `id` (client-supplied UUID string) and
`name` (human-readable label). If `id` is empty the server generates a UUID.

### Mutations

| Method | Effect |
|---|---|
| `set_enabled(handle, enabled)` | Show / hide |
| `delete(handle)` | Remove from scene |
| `set_name(handle, name)` | Rename (clears if `""`) |
| `set_position(handle, x, y)` | Move centre |
| `set_orientation(handle, angle_deg)` | Rotate (CCW degrees) |
| `set_fill_color(handle, r, g, b, a=1)` | Fill colour |
| `set_alpha(handle, opacity)` | Global opacity `[0, 1]` |
| `set_outline_color(handle, r, g, b, a=1)` | Outline colour |
| `set_outline_width(handle, line_width)` | Outline stroke width |
| `set_rect_size(handle, width, height)` | Resize a Rect |
| `set_circle_radius(handle, radius)` | Resize a Circle |
| `set_ellipse_size(handle, width, height)` | Resize an Ellipse |
| `set_grating_phase(handle, phase)` | Phase `[0, 1]` |
| `set_grating_sf(handle, sf)` | Spatial frequency in cycles/pixel |
| `set_grating_contrast(handle, contrast)` | Contrast `[0, 1]` |
| `set_grating_waveform(handle, waveform)` | `GratingTexture.{SIN,SQR,SAW,TRI}` |
| `set_grating_mask(handle, mask)` | `GratingMask.{NONE,CIRCLE,GAUSS,HANN,RAISED_COS}` |
| `set_grating_drift_speed(handle, speed)` | Drift rate in cycles/second |
| `set_grating_drift_decoupled(handle, bool)` | Decouple drift direction from stripe angle |
| `set_grating_drift_angle(handle, angle_deg)` | Drift direction when decoupled |
| `set_grating_fore_color(handle, r, g, b, a=1)` | Peak colour |
| `set_grating_back_color(handle, r, g, b, a=1)` | Trough colour |
| `set_grating_opacity(handle, opacity)` | Grating global opacity |
| `set_text(handle, text)` | Replace the displayed string |
| `set_text_color(handle, r, g, b, a=1)` | Text colour |
| `query(handle)` | Returns `StimulusInfo` |

### Coordinate system

Origin at screen centre, Y-up, units in pixels.

## `conn.system` — SystemClient

| Method | Effect |
|---|---|
| `query_server_info()` | Returns `ServerInfo(width, height, frame_rate, version)` |
| `set_background(r, g, b, a=1)` | Background clear colour |
| `set_deferred_mode(active, *, cancel=False)` | Enter / exit deferred (frame-batched) mode |
| `delete_all()` | Remove all stimuli |
| `set_all_enabled(enabled)` | Show / hide all stimuli |

## `vstimd.psychopy` — PsychoPy-compatible layer

Drop-in replacement for `psychopy.visual`:

```python
# Before
from psychopy import visual

# After
from vstimd import psychopy as visual
```

The only required addition is `address=` on `Window`:

```python
win = visual.Window(size=(1920, 1080), units='pix',
                    address='tcp://192.168.1.10:5555')
circ = visual.Circle(win, radius=50, fillColor='red')
rect = visual.Rect(win, width=200, height=100, fillColor=(-1, 1, -1))
grat = visual.GratingStim(win, sf=0.05, mask='circle')
circ.draw()
win.flip()
```

### Implemented classes

| Class | Notes |
|---|---|
| `Window` | Owns the `Connection`; `flip()` flushes the command queue |
| `Rect` | `create_rect`, position, size, fill color, orientation, alpha |
| `Circle` | `create_circle`, position, radius, fill color, orientation, alpha |
| `GratingStim` | `create_grating`, all grating parameters; `mask` accepts `'circle'`, `'gauss'`, `'raisedCos'` |

All constructor arguments from `psychopy.visual` are accepted. Parameters that
have no server-side equivalent (`autoLog`, `depth`, `interpolate`, etc.) are
accepted and silently ignored for drop-in compatibility.

### Deferred (frame-buffer) mode

By default (`deferred=True`) property changes are queued locally and flushed
atomically on `win.flip()`, aligned to the next vsync. Set `deferred=False`
to send each command immediately.

### Color formats accepted

Named strings (`'red'`), hex strings (`'#ff0000'`), PsychoPy `rgb` tuples
`(-1..1)`, plain `0..1` tuples, `rgb255` tuples, and scalar greyscale values.

## Regenerating protobuf stubs

```bash
cd client/python
make proto   # requires grpcio-tools in the dev dependency group
```

## Tests

```bash
cd client/python

# Unit tests (no server required)
uv run pytest tests/unit/ -v

# E2E against the null renderer (builds server binary automatically)
make test-e2e-null

# E2E against a real running server
VSTIM_SERVER_ADDR=tcp://192.168.1.10:5555 make test-e2e
```
