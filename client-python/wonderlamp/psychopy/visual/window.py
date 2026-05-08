from __future__ import annotations

from ..._connection import Connection
from ._colors import normalize_color
from ._types import ColorInput, MonitorProtocol


class Window:
    """Connection to wonderlamp_server, owning the ZMQ socket.

    Parameters mirror psychopy.visual.Window.  The only required addition is
    *address* to specify the server endpoint.
    """

    def __init__(
        self,
        size: tuple[int, int] = (800, 600),
        color: ColorInput = (-1, -1, -1),
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
        self._queue: list[tuple[str, ...]] = []
        self._to_draw_once: list[int] = []

    def _dispatch(self, method: str, *args: float | int | bool) -> None:
        if self.deferred:
            self._queue.append((method, *args))
        else:
            getattr(self._conn, method)(*args)

    def _resolve_units(self, stim_units: str) -> str:
        return stim_units if stim_units else self.units

    def flip(self) -> None:
        """Send all staged commands to the server (deferred mode)."""
        if not self.deferred:
            return
        for h in self._to_draw_once:
            self._conn.set_enabled(h, True)
        for item in self._queue:
            method, *args = item
            getattr(self._conn, method)(*args)
        self._queue.clear()
        for h in self._to_draw_once:
            self._conn.set_enabled(h, False)
        self._to_draw_once.clear()

    def close(self) -> None:
        self._conn.close()

    def __enter__(self) -> Window:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
