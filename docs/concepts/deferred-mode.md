# Deferred Mode

Deferred mode lets you batch multiple stimulus changes so they all become visible on exactly
the same frame — no tearing, no partial updates.

Without deferred mode, each command is applied immediately and may be visible in a different
frame than related commands sent just after it.

## How it works

1. **Begin** — `set_deferred_mode(active=True)`. The server snapshots all current stimulus
   state into staging fields and begins writing subsequent changes there.
2. **Send commands** — position, colour, enable/disable changes are staged, not applied.
3. **Commit** — `set_deferred_mode(active=False)`. The server sets a `pending_flip` flag.
4. **Next frame** — the render loop atomically promotes all staged changes to live state before
   drawing. Every change in the batch is visible from that frame onward.

## Python usage

```python
from vstimd.stimuli import Vec2, Color

conn.system.set_deferred_mode(active=True)

conn.stimuli.set_enabled(h1, True)
conn.stimuli.set_position(h2, pos=Vec2(100, 0))
conn.stimuli.set_fill_color(h3, color=Color(1.0, 0.0, 0.0))

conn.system.set_deferred_mode(active=False)   # commit — all three appear on same frame
```

To discard staged changes without committing:

```python
conn.system.set_deferred_mode(active=False, cancel=True)
```

## Guarantees

- All commands sent between begin and commit are applied atomically on the same vsync.
- The server is single-flip: only one deferred batch can be in progress at a time.
- Commands sent outside a deferred block are applied as soon as the render thread's read lock
  is available (typically within one frame).

## Typical use cases

- **Flicker paradigms** — toggle multiple stimuli on/off together
- **Position updates** — move multiple stimuli simultaneously (e.g. an array)
- **Colour changes** — change a set of stimuli at a known frame boundary
- **Reveal** — create several stimuli hidden, then reveal them all at once
