from __future__ import annotations

from typing import Callable

from vstimd._proto import service_pb2, system_pb2
from vstimd._proto.vstimd.v1 import color_pb2
from ._models import ServerInfo, ServerVersion


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class SystemClient:
    """Scene-wide commands and server queries (system target)."""

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
        return ServerInfo(
            width=info.width,
            height=info.height,
            frame_rate=info.frame_rate,
            version=ServerVersion(v.major, v.minor, v.patch),
        )

    # ── Scene mutations ───────────────────────────────────────────────────────

    def set_background(self, r: float, g: float, b: float, a: float = 1.0) -> None:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_background=system_pb2.SetBackgroundRequest(
                color=color_pb2.Color(r=r, g=g, b=b, a=a)
            ),
        )
        self._send(req)

    def set_deferred_mode(self, active: bool, *, cancel: bool = False) -> None:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_deferred_mode=system_pb2.SetDeferredModeRequest(active=active, cancel=cancel),
        )
        self._send(req)

    def delete_all(self) -> None:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            delete_all=system_pb2.DeleteAllRequest(),
        )
        self._send(req)

    def set_all_enabled(self, enabled: bool) -> None:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            set_all_enabled=system_pb2.SetAllEnabledRequest(enabled=enabled),
        )
        self._send(req)
