# vstimd Python Client

Python client for the `vstimd` visual stimulus server.  Communicates over
ZeroMQ using protobuf encoding.

## Install

```bash
pip install vstimd
```

Or with [uv](https://docs.astral.sh/uv/):

```bash
uv add vstimd
```

---

## Two APIs

vstimd exposes two complementary Python APIs.  Choose the one that fits your
use case; they can also be mixed in the same script.

### Command API

The primary interface.  Thin wrappers around the protobuf/ZMQ protocol —
each method call maps directly to a server command.

```python
from vstimd import Connection
from vstimd.stimuli import Vec2, Color

with Connection() as conn:
    h = conn.stimuli.shapes.create_rect(
        pos=Vec2(-200, 0), width=300, height=200,
        color=Color(1.0, 0.0, 0.0),
    )
    conn.stimuli.set_enabled(h, False)
    conn.stimuli.delete(h)
```

Use the command API when you need:

- **Animations** — pre-uploaded keyframe sequences executed on the server with
  vsync-accurate timing
- **VTL** (Virtual Trigger Lines) — frame-precise conditional logic without
  round-trip latency
- Full control over stimulus handles and server-side state

These are the mechanisms that give vstimd its strong frame-timing guarantees.

### PsychoPy-compatible layer

Object-oriented wrappers built on top of the command API.  Designed as a
drop-in replacement for `psychopy.visual` so that existing experiment scripts
require minimal changes.

```python
from vstimd.psychopy import visual

win = visual.Window(address='tcp://192.168.1.10:5555')
circ = visual.Circle(win, radius=50, fillColor='red')
circ.draw()
win.flip()
```

```{admonition} v0.1 limitation
As of v0.1 the PsychoPy-compatible layer does **not** expose the animation or
VTL systems.  Stimulus updates go through Python round-trips, which are
subject to OS scheduling jitter.  For experiments where frame timing is
critical, use the command API directly.
```

See {doc}`psychopy_users` for the full migration guide, supported classes, and
unit system documentation.

---

## Next steps

- {doc}`psychopy_users` — migrating from `psychopy.visual`
- {doc}`api/connection` — `Connection` and transport options
- {doc}`api/stimuli/index` — stimulus clients and parameter models
- {doc}`api/animations` — animation and VTL API
- {doc}`api/index` — command API reference
- {doc}`api/psychopy/index` — PsychoPy API reference

```{toctree}
:maxdepth: 2
:hidden:

psychopy_users
api/index
api/psychopy/index
```
