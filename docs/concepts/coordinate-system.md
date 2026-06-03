# Coordinate System

All 2-D stimulus positions use a **pixel-space coordinate system**:

- **Origin** at the screen centre
- **X** increases to the right
- **Y** increases upward
- **Units** are pixels

```
                  +Y
                   │
                   │
    ───────────────┼───────────────  X
                   │
                   │
                  -Y
```

A rectangle at `(x=0, y=0)` is centred on screen. A rectangle at `(x=200, y=0)` is 200 pixels
to the right of centre.

## Examples

| Position | Meaning |
|---|---|
| `x=0, y=0` | Screen centre |
| `x=500, y=0` | 500 px right of centre |
| `x=0, y=-300` | 300 px below centre |
| `x=-100, y=200` | Upper-left quadrant |

## Screen size

Query the display dimensions at runtime:

=== "Python"

    ```python
    info = conn.system.query_server_info()
    w, h = info.display_width, info.display_height
    # Top-right corner of the screen:
    x_max = w / 2
    y_max = h / 2
    ```

## Orientation

Stimulus rotation (`orientation_deg`) is measured in degrees, **counter-clockwise** from the
positive X axis. A rectangle with `orientation_deg=45` is rotated 45° CCW.

## Notes

- The coordinate system matches PsychoPy's `units="pix"` mode.
- The server's internal vertex shader converts pixel coordinates to Vulkan NDC at draw time.
  Clients never need to think about NDC.
