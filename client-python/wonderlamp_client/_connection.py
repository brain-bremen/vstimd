"""Low-level ZMQ + protobuf connection to wonderlamp_server.

Wraps a ZMQ REQ socket and exposes the three commands currently implemented
by the server: :meth:`create_rect`, :meth:`set_enabled`, :meth:`delete`.

Example::

    with Connection() as conn:
        handle = conn.create_rect(x=0, y=0, width=200, height=100,
                                  r=1.0, g=0.0, b=0.0)
        conn.set_enabled(handle, False)
        conn.delete(handle)
"""

from __future__ import annotations

import zmq  # type: ignore[import]

from ._proto import wonderlamp_pb2 as pb


class Connection:
    """ZMQ REQ socket connected to a single wonderlamp_server instance.

    Parameters
    ----------
    address:
        ZMQ endpoint of the server (default ``tcp://localhost:5555``).
    """

    def __init__(self, address: str = "tcp://localhost:5555") -> None:
        self._ctx = zmq.Context.instance()
        self._sock = self._ctx.socket(zmq.REQ)
        self._sock.setsockopt(zmq.LINGER, 0)
        self._sock.connect(address)

    # ── internal ──────────────────────────────────────────────────────────────

    def _send(self, req: pb.Request) -> pb.Response:
        self._sock.send(req.SerializeToString())
        raw = self._sock.recv()
        resp = pb.Response()
        resp.ParseFromString(raw)
        if resp.error:
            raise RuntimeError(f"server error: {resp.error}")
        return resp

    # ── commands ──────────────────────────────────────────────────────────────

    def create_rect(
        self,
        *,
        x: float = 0.0,
        y: float = 0.0,
        width: float = 100.0,
        height: float = 100.0,
        r: float = 1.0,
        g: float = 1.0,
        b: float = 1.0,
        a: float = 1.0,
    ) -> int:
        """Create a rectangle stimulus and return its handle.

        Coordinates are in pixel-space with the origin at the screen centre
        and Y pointing up (matching the server convention).
        """
        req = pb.Request(
            handle=0,
            create_rect=pb.CreateRect(
                center=pb.Vec2(x=x, y=y),
                width=width,
                height=height,
                fill=pb.Color(r=r, g=g, b=b, a=a),
            ),
        )
        return self._send(req).handle

    def set_enabled(self, handle: int, enabled: bool) -> None:
        """Enable or disable the stimulus identified by *handle*."""
        req = pb.Request(
            handle=handle,
            set_enabled=pb.SetEnabled(enabled=enabled),
        )
        self._send(req)

    def delete(self, handle: int) -> None:
        """Permanently remove the stimulus identified by *handle*."""
        req = pb.Request(handle=handle, delete=pb.Delete())
        self._send(req)

    # ── context manager ───────────────────────────────────────────────────────

    def __enter__(self) -> "Connection":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def close(self) -> None:
        """Close the ZMQ socket."""
        self._sock.close()
