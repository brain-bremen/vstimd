"""Shared type aliases for the wonderlamp.visual package."""

from __future__ import annotations

from typing import Protocol, runtime_checkable

# Any color value PsychoPy accepts
ColorInput = str | tuple[float, ...] | list[float] | float | int | None

# (r, g, b, a) normalised to 0..1
Rgba = tuple[float, float, float, float]

# A 2-D position or size as a sequence of two floats
Vec2 = tuple[float, float] | list[float]


@runtime_checkable
class MonitorProtocol(Protocol):
    """Minimal interface of a psychopy.monitors.Monitor used for deg/cm conversion."""

    def deg2pix(self, deg: float) -> float: ...
    def cm2pix(self, cm: float) -> float: ...
