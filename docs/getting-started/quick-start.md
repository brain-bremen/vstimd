# Quick Start

## 1. Start the server

```sh
# Fullscreen (auto-detects DRM or desktop)
cargo run --release

# Windowed, for development
cargo run --release -- --windowed 1280x720

# No display — ZMQ server only (for testing without a monitor)
cargo run --release -- --null
```

Press **D** to spawn demo stimuli, **F1** to toggle the debug overlay, **Esc** to exit.

## 2. Send your first stimulus

=== "Python"

    ```sh
    cd client/python
    uv run examples/flash_rects.py
    ```

    Or from a script:

    ```python
    from vstimd import Connection

    with Connection() as conn:         # default: tcp://localhost:5555
        # Create a red rectangle centred on screen
        h = conn.stimuli.create_rect(
            x=0, y=0, width=300, height=150,
            r=1.0, g=0.0, b=0.0,
        )
        input("Press Enter to remove...")
        conn.stimuli.delete(h)
    ```

=== "MATLAB"

    ```matlab
    conn = vstimd.Connection();   % default: tcp://localhost:5555
    h = conn.stimuli.create_rect('x', 0, 'y', 0, ...
                                 'width', 300, 'height', 150, ...
                                 'r', 1.0, 'g', 0.0, 'b', 0.0);
    input('Press Enter to remove...');
    conn.stimuli.delete(h);
    conn.close();
    ```

## 3. Deferred mode

Use deferred mode to make multiple changes visible on the exact same frame:

=== "Python"

    ```python
    with Connection() as conn:
        h1 = conn.stimuli.create_rect(x=-200, y=0, width=100, height=100, r=1, g=0, b=0)
        h2 = conn.stimuli.create_rect(x= 200, y=0, width=100, height=100, r=0, g=0, b=1)

        with conn.system.deferred():
            conn.stimuli.set_enabled(h1, True)
            conn.stimuli.set_enabled(h2, True)
        # Both stimuli appear on the same frame
    ```

## 4. Query server info

=== "Python"

    ```python
    with Connection() as conn:
        info = conn.system.query_server_info()
        print(info.display_width, info.display_height, info.refresh_rate_hz)
    ```

## Next steps

- [Coordinate system](../concepts/coordinate-system.md) — pixel space, origin, Y-up
- [Deferred mode](../concepts/deferred-mode.md) — atomic multi-stimulus frame flips
- [Python API reference](../api/python/index.md)
- [Bare-metal Linux](bare-metal.md) — running without a compositor on Jetson/Pi
