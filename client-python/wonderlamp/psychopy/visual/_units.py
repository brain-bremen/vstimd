"""Unit conversion: stimulus coordinates → pixels sent to the server.

The server always works in pixels with origin at screen centre, Y-up.
"""

from __future__ import annotations

from ._types import MonitorProtocol, Vec2


def to_pixels(
    value: float | Vec2,
    units: str,
    win_size: tuple[int, int],
    monitor: MonitorProtocol | None = None,
) -> float | tuple[float, float]:
    """Convert a scalar or (x, y) pair from *units* to pixels."""
    scalar = isinstance(value, (int, float))
    x, y = (float(value), float(value)) if scalar else (float(value[0]), float(value[1]))  # type: ignore[index]
    w, h = win_size

    if units in ("pix", ""):
        px, py = x, y
    elif units == "norm":
        px, py = x * w / 2.0, y * h / 2.0
    elif units == "height":
        px, py = x * h, y * h
    elif units in ("deg", "cm"):
        if monitor is None:
            raise ValueError(f"units='{units}' requires a monitor object")
        if units == "deg":
            px, py = monitor.deg2pix(x), monitor.deg2pix(y)
        else:
            px, py = monitor.cm2pix(x), monitor.cm2pix(y)
    else:
        raise ValueError(f"Unknown units: {units!r}")

    return float(px) if scalar else (px, py)
