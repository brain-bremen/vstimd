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

The `Connection.system` client exposes a context manager that handles begin/commit:

```python
with conn.system.deferred():
    conn.stimuli.set_enabled(h1, True)
    conn.stimuli.set_position(h2, x=100, y=0)
    conn.stimuli.set_fill_color(h3, r=1.0, g=0.0, b=0.0)
# All three changes appear on the same frame
```

If an exception is raised inside the block, the deferred mode is cancelled (staged changes
are discarded) and the server returns to its previous state.

## Manual control

=== "Python"

    ```python
    conn.system.set_deferred_mode(active=True)

    conn.stimuli.set_enabled(h1, True)
    conn.stimuli.set_enabled(h2, False)

    conn.system.set_deferred_mode(active=False)  # commit
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
