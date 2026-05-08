"""Color normalization: any PsychoPy-style color input → (r, g, b, a) floats in 0..1.

PsychoPy's default colorSpace='rgb' uses -1..1 rather than 0..1.  The midpoint
0.0 is mid-grey, matching OpenGL convention and making contrast arithmetic clean
(color * contrast).  We convert to 0..1 before sending over the wire.
"""

from __future__ import annotations

from ._types import ColorInput, Rgba

_NAMED: dict[str, tuple[float, float, float]] = {
    "white":   (1.0, 1.0, 1.0),
    "black":   (0.0, 0.0, 0.0),
    "red":     (1.0, 0.0, 0.0),
    "green":   (0.0, 1.0, 0.0),
    "blue":    (0.0, 0.0, 1.0),
    "yellow":  (1.0, 1.0, 0.0),
    "cyan":    (0.0, 1.0, 1.0),
    "magenta": (1.0, 0.0, 1.0),
    "gray":    (0.5, 0.5, 0.5),
    "grey":    (0.5, 0.5, 0.5),
    "orange":  (1.0, 0.647, 0.0),
    "purple":  (0.502, 0.0, 0.502),
    "pink":    (1.0, 0.753, 0.796),
}


def normalize_color(
    color: ColorInput,
    color_space: str = "rgb",
    alpha: float = 1.0,
) -> Rgba | None:
    """Return (r, g, b, a) in 0..1, or None for transparent/no-color."""
    if color is None:
        return None

    if isinstance(color, str):
        if color.startswith("#"):
            h = color.lstrip("#")
            if len(h) == 3:
                h = h[0]*2 + h[1]*2 + h[2]*2
            r, g, b = (int(h[i:i+2], 16) / 255.0 for i in (0, 2, 4))
            return (r, g, b, alpha)
        name = color.lower()
        if name in _NAMED:
            r, g, b = _NAMED[name]
            return (r, g, b, alpha)
        raise ValueError(f"Unknown color name: {color!r}")

    # Single scalar → greyscale in PsychoPy -1..1 convention
    if isinstance(color, (int, float)):
        v = max(0.0, min(1.0, (float(color) + 1.0) / 2.0))
        return (v, v, v, alpha)

    seq = tuple(float(x) for x in color)
    if color_space == "rgb255":
        r, g, b = (x / 255.0 for x in seq[:3])
    elif color_space == "rgb1":
        r, g, b = seq[:3]
    else:
        # PsychoPy default 'rgb': -1..1 → 0..1
        r, g, b = ((x + 1.0) / 2.0 for x in seq[:3])

    r, g, b = (max(0.0, min(1.0, x)) for x in (r, g, b))
    a = float(seq[3]) if len(seq) >= 4 else alpha
    return (r, g, b, a)
