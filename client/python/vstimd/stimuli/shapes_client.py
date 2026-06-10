from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import color_pb2, vec2_pb2
from vstimd._proto.vstimd.v1.stimuli import (
    circle_pb2,
    ellipse_pb2,
    rect_pb2,
    shapes_pb2,
)

from .color import Color
from .shapes_models import ShapeDrawMode, _SHAPE_DRAW_MODE_TO_PROTO
from .vec import Vec2

_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class ShapesClient:
    """Create and mutate rect, circle, and ellipse stimuli."""

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    # ── Creation ──────────────────────────────────────────────────────────────

    def create_rect(
        self,
        *,
        pos: Vec2 = Vec2(0.0, 0.0),
        width: float = 100.0,
        height: float = 100.0,
        color: Color = Color(1.0, 1.0, 1.0),
        name: str = "",
        id: str = "",
    ) -> StimulusHandle:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_rect=rect_pb2.CreateRectRequest(
                center=vec2_pb2.Vec2(x=pos.x, y=pos.y),
                width=width,
                height=height,
                fill_color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                name=name,
                id=id,
            ),
        )
        return StimulusHandle(self._send(req).handle)

    def create_circle(
        self,
        *,
        pos: Vec2 = Vec2(0.0, 0.0),
        radius: float = 50.0,
        color: Color = Color(1.0, 1.0, 1.0),
        name: str = "",
        id: str = "",
    ) -> StimulusHandle:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_circle=circle_pb2.CreateCircleRequest(
                center=vec2_pb2.Vec2(x=pos.x, y=pos.y),
                radius=radius,
                fill_color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                name=name,
                id=id,
            ),
        )
        return StimulusHandle(self._send(req).handle)

    def create_ellipse(
        self,
        *,
        pos: Vec2 = Vec2(0.0, 0.0),
        width: float = 100.0,
        height: float = 50.0,
        angle: float = 0.0,
        color: Color = Color(1.0, 1.0, 1.0),
        name: str = "",
        id: str = "",
    ) -> StimulusHandle:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_ellipse=ellipse_pb2.CreateEllipseRequest(
                center=vec2_pb2.Vec2(x=pos.x, y=pos.y),
                width=width,
                height=height,
                angle=angle,
                fill_color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                name=name,
                id=id,
            ),
        )
        return StimulusHandle(self._send(req).handle)

    def set_rect_size(
        self, handle: StimulusHandle, width: float, height: float
    ) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_rect_size=rect_pb2.SetRectSizeRequest(width=width, height=height),
            )
        )

    def set_circle_radius(self, handle: StimulusHandle, radius: float) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_circle_radius=circle_pb2.SetCircleRadiusRequest(radius=radius),
            )
        )

    def set_ellipse_size(
        self, handle: StimulusHandle, width: float, height: float
    ) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_ellipse_size=ellipse_pb2.SetEllipseSizeRequest(
                    width=width, height=height
                ),
            )
        )

    def set_draw_mode(self, handle: StimulusHandle, mode: ShapeDrawMode) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_draw_mode=shapes_pb2.SetDrawModeRequest(
                    mode=_SHAPE_DRAW_MODE_TO_PROTO[mode],
                ),
            )
        )

    def set_outline_color(self, handle: StimulusHandle, color: Color) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_outline_color=shapes_pb2.SetOutlineColorRequest(
                    color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                ),
            )
        )

    def set_outline_width(self, handle: StimulusHandle, line_width: float) -> None:
        self._send(
            service_pb2.Request(
                stimulus=handle,
                set_outline_width=shapes_pb2.SetOutlineWidthRequest(
                    line_width=line_width
                ),
            )
        )
