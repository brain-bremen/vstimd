from __future__ import annotations

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import color_pb2
from vstimd._proto.vstimd.v1.stimuli import (
    query_pb2,
    shared_set_requests_pb2,
)

from ._grating import GratingClient
from ._shapes import ShapesClient, _SendFn
from ._text import TextClient
from .stimuli_models import Color, StimulusInfo, Vec2


class StimuliClient:
    """Top-level stimuli client; groups subclients by stimulus family."""

    def __init__(self, send: _SendFn) -> None:
        self.shapes = ShapesClient(send)
        self.grating = GratingClient(send)
        self.text = TextClient(send)
        self._send = send

    # ── Generic mutations ──────────────────────────────────────────────────────

    def set_name(self, handle: StimulusHandle, name: str) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_name=shared_set_requests_pb2.SetNameRequest(name=name),
            )
        )

    def set_enabled(self, handle: StimulusHandle, enabled: bool) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_enabled=shared_set_requests_pb2.SetEnabledRequest(enabled=enabled),
            )
        )

    def delete(self, handle: StimulusHandle) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                delete=shared_set_requests_pb2.DeleteRequest(),
            )
        )

    def set_position(self, handle: StimulusHandle, pos: Vec2) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_position=shared_set_requests_pb2.SetPositionRequest(
                    x=pos.x, y=pos.y
                ),
            )
        )

    def set_orientation(self, handle: StimulusHandle, angle_deg: float) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_orientation=shared_set_requests_pb2.SetOrientationRequest(
                    angle_deg=angle_deg
                ),
            )
        )

    def set_fill_color(self, handle: StimulusHandle, color: Color) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_fill_color=shared_set_requests_pb2.SetFillColorRequest(
                    color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                ),
            )
        )

    def set_alpha(self, handle: StimulusHandle, opacity: float) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_alpha=shared_set_requests_pb2.SetAlphaRequest(opacity=opacity),
            )
        )

    # ── Query ──────────────────────────────────────────────────────────────────

    def query(self, handle: StimulusHandle) -> StimulusInfo:
        """Return current server-side properties for the given stimulus handle."""
        req = service_pb2.Request(
            stimulus=handle,
            query_stimulus=query_pb2.QueryStimulusRequest(),
        )
        return StimulusInfo.from_proto(self._send(req).stimulus_info)
