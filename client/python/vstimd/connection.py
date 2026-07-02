from __future__ import annotations

import time
import zmq  # type: ignore[import]

from vstimd._proto import service_pb2
from vstimd.stimuli import StimuliClient
from vstimd.system import SystemClient
from vstimd.vtl import VtlClient
from vstimd.animations import AnimationClient
from vstimd.config import ConfigClient
from vstimd.exceptions import (
    VstimdError,
    HandleNotFoundError,
    WrongStimulusTypeError,
    WrongTargetError,
    CreationFailedError,
    InvalidArgumentError,
    NotSupportedError,
    NotReadyError,
    UnknownServerError,
    ConfigNotFoundError,
    ConfigIoError,
    ConfigFormatError,
    ConfigVersionError,
    ConfigAlreadyExistsError,
)

_ERROR_CODE_MAP: dict[int, type[VstimdError]] = {
    service_pb2.ERROR_CODE_UNKNOWN: UnknownServerError,
    service_pb2.ERROR_CODE_HANDLE_NOT_FOUND: HandleNotFoundError,
    service_pb2.ERROR_CODE_WRONG_STIMULUS_TYPE: WrongStimulusTypeError,
    service_pb2.ERROR_CODE_WRONG_TARGET: WrongTargetError,
    service_pb2.ERROR_CODE_CREATION_FAILED: CreationFailedError,
    service_pb2.ERROR_CODE_INVALID_ARGUMENT: InvalidArgumentError,
    service_pb2.ERROR_CODE_NOT_SUPPORTED: NotSupportedError,
    service_pb2.ERROR_CODE_NOT_READY: NotReadyError,
    service_pb2.ERROR_CODE_FILE_NOT_FOUND: ConfigNotFoundError,
    service_pb2.ERROR_CODE_FILE_IO: ConfigIoError,
    service_pb2.ERROR_CODE_FILE_FORMAT: ConfigFormatError,
    service_pb2.ERROR_CODE_UNSUPPORTED_VERSION: ConfigVersionError,
    service_pb2.ERROR_CODE_FILE_ALREADY_EXISTS: ConfigAlreadyExistsError,
}


class Connection:
    """ZMQ REQ socket connected to a single vstimd instance.

    Client sub-objects are available as attributes and cover the full command
    API:

    * ``stimuli`` — :class:`~vstimd.stimuli.StimuliClient`: create and mutate stimuli
    * ``system`` — :class:`~vstimd.system.SystemClient`: scene-wide commands and server queries
    * ``vtl`` — :class:`~vstimd.VtlClient`: Virtual Trigger Line control
    * ``animations`` — :class:`~vstimd.AnimationClient`: frame-accurate animation sequences
    * ``config`` — :class:`~vstimd.config.ConfigClient`: save, load, and retrieve named scene configs

    Example::

        with Connection() as conn:
            h = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0), width=200, height=100,
                                                color=Color(1, 0, 0))
            conn.vtl.set_line_name(0, 0, VtlKind.OUTPUT, "frame_sync")
            anim = conn.animations.create_flash(h, duration_ms=500)
            conn.animations.arm(anim)

    Parameters
    ----------
    address:
        ZMQ endpoint of the server (default ``tcp://localhost:5555``).
    """

    def __init__(
        self,
        address: str = "tcp://localhost:5555",
        *,
        wait_ready: bool = False,
        ready_timeout_s: float = 30.0,
    ) -> None:
        self._address = address
        self._ctx = zmq.Context.instance()
        self._sock = self._ctx.socket(zmq.REQ)
        self._sock.setsockopt(zmq.LINGER, 0)
        self._sock.connect(address)
        self.stimuli = StimuliClient(self._send)
        self.system = SystemClient(self._send)
        self.vtl = VtlClient(self._send)
        self.animations = AnimationClient(
            self._send,
            fps_getter=lambda: self.system.query_server_info().frame_rate,
        )
        self.config = ConfigClient(self._send)
        if wait_ready:
            self.wait_until_ready(timeout_s=ready_timeout_s)

    def _send(self, req: service_pb2.Request) -> service_pb2.Response:
        self._sock.send(req.SerializeToString())
        raw = self._sock.recv()
        resp = service_pb2.Response()
        resp.ParseFromString(raw)
        if resp.code != service_pb2.ERROR_CODE_OK:
            exc_type = _ERROR_CODE_MAP.get(resp.code, UnknownServerError)
            raise exc_type(resp.error or f"server error code {resp.code}")
        return resp

    def __enter__(self) -> "Connection":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def wait_until_ready(
        self,
        timeout_s: float = 30.0,
        *,
        retry_interval_s: float = 0.5,
    ) -> None:
        """Block until the server is up and has rendered at least one frame.

        Retries the ZMQ connection if the server is not yet running.
        Raises ``TimeoutError`` if the server is not ready within *timeout_s*.
        """
        deadline = time.monotonic() + timeout_s
        attempt_ms = max(1, int(retry_interval_s * 1000))

        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError(f"vstimd server not ready after {timeout_s}s")
            self._sock.setsockopt(zmq.RCVTIMEO, attempt_ms)
            try:
                self.system.wait_for_frames(1)
                return
            except zmq.Again:
                self._sock.close()
                self._sock = self._ctx.socket(zmq.REQ)
                self._sock.setsockopt(zmq.LINGER, 0)
                self._sock.connect(self._address)
            finally:
                self._sock.setsockopt(zmq.RCVTIMEO, -1)

    def close(self) -> None:
        """Close the ZMQ socket."""
        self._sock.close()
