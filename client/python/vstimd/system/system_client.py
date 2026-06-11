from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2, system_pb2
from vstimd._proto.vstimd.v1 import color_pb2
from vstimd.response import ServerResponse
from vstimd.stimuli.color import Color
from .system_models import ServerInfo, ServerVersion, StimulusListEntry


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class SystemClient:
    """Scene-wide commands and server queries.

    Accessed as ``conn.system`` on a :class:`~vstimd.Connection` instance.

    Example::

        with Connection() as conn:
            info = conn.system.query_server_info()
            print(info.width, info.height, info.frame_rate)
            conn.system.set_background(0.0, 0.0, 0.0)
    """

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    # ── Queries ───────────────────────────────────────────────────────────────

    def query_server_info(self) -> ServerInfo:
        """Query server display properties and version."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            query_server_info=system_pb2.QueryServerInfoRequest(),
        )
        resp = self._send(req)
        info = resp.server_info
        v = info.version
        bg = info.background_color
        return ServerInfo(
            width=info.width,
            height=info.height,
            frame_rate=info.frame_rate,
            version=ServerVersion(v.major, v.minor, v.patch),
            background_color=Color(r=bg.r, g=bg.g, b=bg.b, a=bg.a),
        )

    def list_stimuli(self) -> list[StimulusListEntry]:
        """Return a list of all currently existing stimuli."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            list_stimuli=system_pb2.ListStimuliRequest(),
        )
        resp = self._send(req)
        return [
            StimulusListEntry(handle=StimulusHandle(e.handle), enabled=e.enabled, id=e.id, name=e.name)
            for e in resp.stimulus_list.entries
        ]

    # ── Scene mutations ───────────────────────────────────────────────────────

    def set_background(self, r: float, g: float, b: float, a: float = 1.0) -> ServerResponse:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_background=system_pb2.SetBackgroundRequest(
                color=color_pb2.Color(r=r, g=g, b=b, a=a)
            ),
        )
        return ServerResponse._from_proto(self._send(req))

    def set_deferred_mode(self, active: bool, *, cancel: bool = False) -> ServerResponse:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_deferred_mode=system_pb2.SetDeferredModeRequest(active=active, cancel=cancel),
        )
        return ServerResponse._from_proto(self._send(req))

    def delete_all(self) -> ServerResponse:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            delete_all=system_pb2.DeleteAllRequest(),
        )
        return ServerResponse._from_proto(self._send(req))

    def set_all_enabled(self, enabled: bool) -> ServerResponse:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_all_enabled=system_pb2.SetAllEnabledRequest(enabled=enabled),
        )
        return ServerResponse._from_proto(self._send(req))

    # ── Timing ───────────────────────────────────────────────────────────────

    def wait_for_frames(self, count: int) -> ServerResponse:
        """Block until `count` additional render frames have completed."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            wait_for_frames=system_pb2.WaitForFramesRequest(count=count),
        )
        return ServerResponse._from_proto(self._send(req))

    def wait_until(self, server_time_ns: int) -> ServerResponse:
        """Block until the server's monotonic clock reaches `server_time_ns`."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            wait_until=system_pb2.WaitUntilRequest(server_time_ns=server_time_ns),
        )
        return ServerResponse._from_proto(self._send(req))
