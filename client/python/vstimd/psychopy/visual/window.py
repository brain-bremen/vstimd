from __future__ import annotations

from typing import Any, Callable

from ..._handles import StimulusHandle
from ..._connection import Connection
from ._types import PsychoPyColor, MonitorProtocol


class Window:
    """Connection to vstimd, owning the ZMQ socket.

    Parameters mirror psychopy.visual.Window.  The only required addition is
    *address* to specify the server endpoint.
    """

    def __init__(
        self,
        size: tuple[int, int] = (800, 600),
        color: PsychoPyColor = (-1, -1, -1),
        units: str = "pix",
        monitor: MonitorProtocol | None = None,
        deferred: bool = True,
        address: str = "tcp://localhost:5555",
        # accepted for psychopy compat, ignored
        fullscr: bool = False,
        screen: int = 0,
        colorSpace: str = "rgb",
        autoLog: bool = False,
    ) -> None:
        self.size = tuple(size)
        self.units = units
        self.monitor = monitor
        self.deferred = deferred
        self.colorSpace = colorSpace

        self._conn = Connection(address)
        self._queue: list[tuple[Callable[..., Any], tuple[Any, ...]]] = []
        self._to_draw_once: list[StimulusHandle] = []

    def _dispatch(self, fn: Callable[..., Any], *args: Any) -> None:
        if self.deferred:
            self._queue.append((fn, args))
        else:
            fn(*args)

    def _resolve_units(self, stim_units: str) -> str:
        return stim_units if stim_units else self.units

    def flip(self) -> None:
        """Send all staged commands to the server (deferred mode)."""
        if not self.deferred:
            return
        for h in self._to_draw_once:
            self._conn.stimuli.shapes.set_enabled(h, True)
        for fn, args in self._queue:
            fn(*args)
        self._queue.clear()
        for h in self._to_draw_once:
            self._conn.stimuli.shapes.set_enabled(h, False)
        self._to_draw_once.clear()

    def close(self) -> None:
        self._conn.close()

    def __enter__(self) -> Window:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
