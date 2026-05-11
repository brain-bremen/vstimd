# `wonderlamp.psychopy` — PsychoPy-compatible visual layer

Drop-in replacement for `psychopy.visual` that renders via `wonderlamp_server`
instead of a local OpenGL context.

## Goal

Existing neuroscience experiments can swap one import line:

```python
# Before
from psychopy import visual

# After
from wonderlamp import psychopy as visual   # or: import wonderlamp.psychopy.visual as visual
```

All constructor arguments, property setters, and drawing methods work unchanged.
The only addition is the `address` parameter on `Window` to specify the server endpoint.

## Design principles

**Flat class hierarchy — no mixins, no shared base.**
Each stimulus class (`Window`, `Rect`, `Circle`, …) is self-contained with all
fields and methods repeated explicitly. 

**Thin wrapper over the core clients.**
Every stimulus class holds a reference to its `Window`, which owns a
`wonderlamp.Connection`. Property setters translate to calls on
`conn.stimuli.*` (e.g. `set_position`, `set_fill_color`, `set_disc_radius`).
No rendering logic lives here — that is entirely the server's job.

**Server-allocated handles.**
`create_rect` / `create_circle` return an integer handle from the server.
Each stimulus object stores this handle and passes it to subsequent mutation
commands. Handle 0 is reserved for system commands.

**Deferred (frame-buffer) mode by default.**
Property changes are queued locally. `win.flip()` flushes the entire queue
in one batch and then disables any one-shot draw handles. Set `deferred=False`
to send each command immediately (useful for debugging or setup code).

## Units

All coordinates are converted to pixels before being sent. The conversion is
handled by `visual/_units.py:to_pixels()`.

| Unit | Conversion |
|---|---|
| `pix` (default) | identity |
| `norm` | `x_px = x × width/2`, `y_px = y × height/2` |
| `height` | `x_px = x × height`, `y_px = y × height` |
| `deg` | requires `monitor.deg2pix()` |
| `cm` | requires `monitor.cm2pix()` |

The server coordinate system matches PsychoPy: origin at screen centre, Y-up.

## Color normalisation

All colors are normalised to `(r, g, b, a)` floats in `0..1` by
`visual/_colors.py:normalize_color()` before being sent over the wire.

| Input format | Example | Notes |
|---|---|---|
| Named string | `'red'`, `'gray'` | Lookup table in `_colors.py` |
| Hex string | `'#ff0000'`, `'#f00'` | 3- and 6-digit forms supported |
| PsychoPy rgb tuple | `(1.0, -1.0, 0.0)` | Default `colorSpace='rgb'`: −1..1 → 0..1 |
| Plain 0..1 tuple | `(1.0, 0.0, 0.0)` | `colorSpace='rgb1'` |
| rgb255 tuple | `(255, 0, 128)` | `colorSpace='rgb255'` |
| Scalar (greyscale) | `0.5` | PsychoPy −1..1 convention |
| `None` | | No color / transparent |

`opacity` is folded into the alpha channel when color is re-sent.

## flip() and draw()

| Mechanism | Behaviour |
|---|---|
| `autoDraw=True` | `set_enabled(True)` sent once; stimulus always visible until disabled |
| `autoDraw=False` | `set_enabled(False)`; stimulus hidden |
| `.draw()` | Adds handle to `win._to_draw_once`; enabled for one flip, then disabled |
| `win.flip()` | Flushes queued commands, enables one-shot handles, sends batch, disables one-shot handles |

This replicates PsychoPy's "draw-once-per-loop-iteration" pattern exactly.

## What is accepted but not forwarded to the server

The following constructor parameters are accepted (for drop-in compatibility)
but have currently no effect:

- `autoLog` — no logging infrastructure in this layer
- `contrast` — stored, not yet sent
- `depth`, `interpolate`, `draggable`, `anchor`, `lineColor`, `lineWidth` — stored or ignored
- `fullscr`, `screen` on `Window` — the server controls display geometry
- `monitor` — only required when `units='deg'` or `'cm'`

## Currently implemented

| Class | Server command(s) used |
|---|---|
| `Window` | owns `Connection`; `flip()` flushes queue |
| `Rect` | `create_rect`, `set_position`, `set_rect_size`, `set_fill_color`, `set_orientation`, `set_enabled`, `set_alpha` |
| `Circle` | `create_circle`, `set_position`, `set_disc_radius`, `set_fill_color`, `set_orientation`, `set_enabled`, `set_alpha` |

## Testing

Tests live in `client-python/tests/`.

API compatibility with `psychopy.visual` is tested via `tests/unit/test_psychopy_compat.py`.
The test uses `inspect.signature` to compare
every `__init__` parameter, public property, and public method of
`wonderlamp.psychopy.visual.{Rect,Circle,Window}` against the real
`psychopy.visual` counterparts. 

### End-to-end tests against the null renderer — no display or GPU required

`tests/e2e/test_psychopy_visual_null.py` starts the server binary in
`--null` mode (ZMQ only, no rendering), then exercises the full round-trip:
create a stimulus, call `query_stimulus`, assert the returned geometry and
color match what was sent. The null binary is built automatically if not
present.

```bash
make test-e2e-null
```

### End-to-end tests against a real server

`tests/e2e/test_e2e.py` and `test_psychopy_visual_null.py` share test cases
via `_psychopy_visual_cases.py`. To run against a real server set
`VSTIM_SERVER_ADDR`:

```bash
make test-e2e
```
