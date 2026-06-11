# vstimd Python client

Python client for the `vstimd` visual stimulus server. Talks to the server
over ZMQ using protobuf encoding.

## Install

```bash
pip install vstimd
```

Or with [uv](https://docs.astral.sh/uv/):

```bash
uv add vstimd
```

### Development install

```bash
cd client/python
uv sync
```

## Quick start

```python
from vstimd import Connection
from vstimd.stimuli import Vec2, Color

with Connection() as conn:
    h = conn.stimuli.shapes.create_rect(pos=Vec2(-200, 0), width=300, height=200,
                                        color=Color(1.0, 0.0, 0.0))
    conn.stimuli.set_enabled(h, False)
    conn.stimuli.delete(h)
    info = conn.system.query_server_info()
    print(info.version)
```

`Connection(address="tcp://localhost:5555")` — default address shown.

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
win = visual.Window(address='tcp://192.168.1.10:5555')
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

By default (`deferred=True`) property changes are sent to the server's deferred
queue immediately; `win.flip()` tells the server to apply the entire queue
atomically before the next vsync. Set `deferred=False` to apply each command
immediately as it arrives.

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
make test

# E2E against the null renderer (builds server binary automatically)
make test-e2e-null

# E2E against a real running server
VSTIM_SERVER_ADDR=tcp://192.168.1.10:5555 make test-e2e
```
