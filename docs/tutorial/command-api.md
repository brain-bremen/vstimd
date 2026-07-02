# Tutorial: The command API

The command API is the **imperative** path: your client sends a command, the server
applies it on the next frame and replies. Use it to build the scene, run your trial
logic, and query state. This tutorial walks from a first stimulus to atomic
multi-stimulus updates.

!!! info "Prerequisites"
    A running server (`cargo run --release`, or `--null` for a headless test — see
    [Quick start](../getting-started/quick-start.md)) and the Python client
    (`cd client/python && uv sync`). The examples use Python; the same commands
    exist in the MATLAB and PsychoPy layers.

## 1. Connect

Every session starts with a `Connection`. It opens a ZMQ REQ socket to the server
and exposes the command namespaces (`stimuli`, `system`, `animations`, `vtl`,
`config`).

```python
from vstimd import Connection

with Connection() as conn:                 # default: tcp://localhost:5555
    info = conn.system.query_server_info()
    print(info.width, info.height, info.frame_rate)
```

For a remote device, pass its address:

```python
with Connection("tcp://stimulus-pc:5555") as conn:
    ...
```

## 2. Create a stimulus

Creating a stimulus returns a **handle** — an opaque id you use to address that
stimulus in later commands. Positions are in **pixels from the screen centre, Y
up** (see [Coordinate system](../concepts/coordinate-system.md)); colours are RGBA
in 0–1.

```python
from vstimd.stimuli import Vec2, Color

with Connection() as conn:
    rect = conn.stimuli.shapes.create_rect(
        pos=Vec2(0, 0), width=300, height=150,
        color=Color(1.0, 0.0, 0.0),        # red
    )
    print(rect)                            # a StimulusHandle
```

The available stimulus types are rectangles, circles, ellipses, gratings, and
text. Each has a matching `create_*` and its own shape/appearance commands — see
the [command reference](../protocol/commands.md).

## 3. Mutate and query

Once you hold a handle, address the stimulus directly. Each call is one command,
applied on the next frame:

```python
conn.stimuli.set_position(rect, Vec2(-200, 100))
conn.stimuli.set_fill_color(rect, Color(0, 0, 1))     # blue
conn.stimuli.set_enabled(rect, True)                  # show it

state = conn.stimuli.query(rect)                       # full current state
print(state.enabled, state.pos)
```

`set_enabled(handle, False)` hides a stimulus without deleting it; `delete(handle)`
removes it entirely.

## 4. The request/response rhythm

Every command is a **synchronous round-trip**: the client blocks until the server
acknowledges. That makes errors explicit — a bad handle raises
`HandleNotFoundError`, a value out of range raises `InvalidArgumentError` (see the
[error codes](../protocol/index.md#error-codes)) — and it makes the API easy to
reason about.

It also means command timing is bounded by the network + OS, not by the display.
That is exactly why the command API is the right tool for **setup and high-level
logic**, and the *wrong* tool for reacting to an external event within a single
frame. For that, arm an [animation](vtl-and-animations.md) instead.

## 5. Deferred mode — atomic multi-stimulus updates

If you enable two stimuli with two separate commands, they may land on two
*different* frames. When several changes must appear **together**, wrap them in a
deferred batch: the server accumulates the changes and flips them all on one frame.

```python
with Connection() as conn:
    left  = conn.stimuli.shapes.create_rect(pos=Vec2(-200, 0), width=100, height=100,
                                            color=Color(1, 0, 0))
    right = conn.stimuli.shapes.create_rect(pos=Vec2(200, 0), width=100, height=100,
                                            color=Color(0, 0, 1))

    conn.system.set_deferred_mode(active=True)
    conn.stimuli.set_enabled(left,  True)
    conn.stimuli.set_enabled(right, True)
    conn.system.set_deferred_mode(active=False)   # both appear on the same frame
```

See [Deferred mode](../concepts/deferred-mode.md) for the full semantics,
including cancelling a batch.

## 6. Scene-wide and query commands

The `system` namespace holds commands that are not addressed to one stimulus:

```python
conn.system.set_background(Color(0.5, 0.5, 0.5))   # grey background
conn.system.set_all_enabled(False)                 # hide everything
conn.system.delete_all()                           # clear the scene

for entry in conn.system.list_stimuli():           # inventory of the scene
    print(entry.handle, entry.type, entry.name, entry.enabled)
```

## Putting it together: a minimal trial loop

```python
import time
from vstimd import Connection
from vstimd.stimuli import Vec2, Color

with Connection("tcp://stimulus-pc:5555") as conn:
    fix = conn.stimuli.shapes.create_circle(pos=Vec2(0, 0), radius=10,
                                            color=Color(1, 1, 1))
    target = conn.stimuli.shapes.create_rect(pos=Vec2(300, 0), width=80, height=80,
                                             color=Color(0, 1, 0))
    conn.stimuli.set_enabled(target, False)

    for trial in range(20):
        conn.stimuli.set_enabled(fix, True)        # fixation on
        time.sleep(0.5)
        conn.stimuli.set_enabled(target, True)     # target on
        time.sleep(0.3)
        conn.system.set_all_enabled(False)         # blank between trials
        time.sleep(0.7)
```

This is fine when the timing is set by *your* logic (the `sleep`s above). The
moment the timing must instead be locked to the display or to an external trigger,
move that part into an [animation](vtl-and-animations.md).

## PsychoPy compatibility

If you have existing PsychoPy code, the `vstimd.psychopy` layer mirrors
`psychopy.visual` on top of the command API — often a one-line import swap:

```python
# from psychopy import visual
from vstimd.psychopy import visual

win  = visual.Window(address="tcp://stimulus-pc:5555")
rect = visual.Rect(win, width=0.5, height=0.25, fillColor="red")
rect.draw()
win.flip()
```

!!! warning "Timing note"
    The PsychoPy-compatible layer drives updates through Python round-trips and, in
    v0.1, does **not** expose the animation or VTL systems. For experiments where
    frame timing is critical, use the command API directly and move timing-critical
    reactions into [animations](vtl-and-animations.md).

## Next

- **[Triggers & animations](vtl-and-animations.md)** — the frame-accurate,
  on-device path.
- **[Command reference](../protocol/commands.md)** — every command and its fields.
- **[Deferred mode](../concepts/deferred-mode.md)** — atomic frame flips in depth.
