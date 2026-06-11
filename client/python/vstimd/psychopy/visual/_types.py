"""Shared type aliases for the vstimd.visual package."""

from __future__ import annotations

from typing import Protocol, runtime_checkable

PsychoPyColor = str | tuple[float, ...] | list[float] | float | int | None
"""Any color value accepted by the PsychoPy-compatible layer.

Accepted forms:

* Named string: ``'red'``, ``'white'``, ``'black'``
* Hex string: ``'#ff0000'``
* PsychoPy ``rgb`` tuple (−1 … 1 per channel): ``(-1, 1, -1)``
* Normalised float tuple (0 … 1 per channel): ``(1.0, 0.0, 0.0)``
* ``rgb255`` tuple (0 … 255 per channel): ``(255, 0, 0)``
* Scalar greyscale: ``0.5`` (float) or ``128`` (int)
* ``None`` — transparent / no fill
"""

PsychoPyVec2 = tuple[float, float] | list[float]
"""A 2-D position or size value accepted by the PsychoPy-compatible layer.

Either a two-element tuple ``(x, y)`` or a two-element list ``[x, y]``.
Units are interpreted according to the ``units`` parameter of the enclosing
stimulus or window (``'pix'``, ``'norm'``, ``'height'``, ``'deg'``, ``'cm'``).
"""


@runtime_checkable
class MonitorProtocol(Protocol):
    """Minimal interface of a psychopy.monitors.Monitor used for deg/cm conversion."""

    def deg2pix(self, deg: float) -> float: ...
    def cm2pix(self, cm: float) -> float: ...
