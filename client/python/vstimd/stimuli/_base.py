from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import color_pb2
from vstimd._proto.vstimd.v1.stimuli import shared_set_requests_pb2, shapes_pb2
from .stimuli_models import Color, DrawMode, Vec2

_SendFn = Callable[[service_pb2.Request], service_pb2.Response]

_DRAW_MODE_TO_PROTO: dict[DrawMode, int] = {
    DrawMode.FILLED:               shapes_pb2.SHAPE_DRAW_MODE_FILLED,
    DrawMode.OUTLINED:             shapes_pb2.SHAPE_DRAW_MODE_OUTLINED,
    DrawMode.FILLED_AND_OUTLINED:  shapes_pb2.SHAPE_DRAW_MODE_FILLED_AND_OUTLINED,
}


class _BaseStimulusClient:
    """Shared mutations that apply to every stimulus type."""

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    def set_name(self, handle: StimulusHandle, name: str) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_name=shared_set_requests_pb2.SetNameRequest(name=name),
        )
        self._send(req)

    def set_enabled(self, handle: StimulusHandle, enabled: bool) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_enabled=shared_set_requests_pb2.SetEnabledRequest(enabled=enabled),
        )
        self._send(req)

    def delete(self, handle: StimulusHandle) -> None:
        req = service_pb2.Request(stimulus=handle, delete=shared_set_requests_pb2.DeleteRequest())
        self._send(req)

    def set_position(self, handle: StimulusHandle, pos: Vec2) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_position=shared_set_requests_pb2.SetPositionRequest(x=pos.x, y=pos.y),
        )
        self._send(req)

    def set_orientation(self, handle: StimulusHandle, angle_deg: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_orientation=shared_set_requests_pb2.SetOrientationRequest(angle_deg=angle_deg),
        )
        self._send(req)

    def set_fill_color(self, handle: StimulusHandle, color: Color) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_fill_color=shared_set_requests_pb2.SetFillColorRequest(
                color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a)
            ),
        )
        self._send(req)

    def set_alpha(self, handle: StimulusHandle, opacity: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_alpha=shared_set_requests_pb2.SetAlphaRequest(opacity=opacity),
        )
        self._send(req)

    def set_draw_mode(self, handle: StimulusHandle, mode: DrawMode) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_draw_mode=shared_set_requests_pb2.SetDrawModeRequest(mode=_DRAW_MODE_TO_PROTO[mode]),
        )
        self._send(req)

    def set_outline_color(self, handle: StimulusHandle, color: Color) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_outline_color=shared_set_requests_pb2.SetOutlineColorRequest(
                color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a)
            ),
        )
        self._send(req)

    def set_outline_width(self, handle: StimulusHandle, line_width: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_outline_width=shared_set_requests_pb2.SetOutlineWidthRequest(line_width=line_width),
        )
        self._send(req)
