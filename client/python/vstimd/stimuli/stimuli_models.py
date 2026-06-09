from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Union

from vstimd._proto.vstimd.v1.stimuli import query_pb2, stimulus_type_pb2

from ._shapes import (
    _PROTO_TO_DRAW_MODE,
    CircleParams,
    EllipseParams,
    RectParams,
    ShapeDrawMode,
)
from .color import Color
from .grating_models import GratingParams
from .vec import Vec2


class StimulusType(Enum):
    UNKNOWN = "unknown"
    RECT = "rect"
    CIRCLE = "circle"
    ELLIPSE = "ellipse"
    BITMAP = "bitmap"
    SHADER = "shader"
    PARTICLE = "particle"
    GRATING = "grating"
    TEXT = "text"
    POLYGON = "polygon"


class LanguageStyle(Enum):
    LTR = "LTR"
    RTL = "RTL"
    ARABIC = "Arabic"


StimulusParams = Union[RectParams, CircleParams, EllipseParams, GratingParams]

_STIMULUS_TYPE_MAP: dict[int, StimulusType] = {
    stimulus_type_pb2.STIMULUS_TYPE_RECT: StimulusType.RECT,
    stimulus_type_pb2.STIMULUS_TYPE_CIRCLE: StimulusType.CIRCLE,
    stimulus_type_pb2.STIMULUS_TYPE_ELLIPSE: StimulusType.ELLIPSE,
    stimulus_type_pb2.STIMULUS_TYPE_BITMAP: StimulusType.BITMAP,
    stimulus_type_pb2.STIMULUS_TYPE_SHADER: StimulusType.SHADER,
    stimulus_type_pb2.STIMULUS_TYPE_PARTICLE: StimulusType.PARTICLE,
    stimulus_type_pb2.STIMULUS_TYPE_GRATING: StimulusType.GRATING,
    stimulus_type_pb2.STIMULUS_TYPE_TEXT: StimulusType.TEXT,
    stimulus_type_pb2.STIMULUS_TYPE_POLYGON: StimulusType.POLYGON,
}


@dataclass
class StimulusInfo:
    stimulus_type: StimulusType
    enabled: bool
    pos: Vec2
    orientation: float
    opacity: float
    fill_color: Color
    outline_color: Color
    outline_width: float
    draw_mode: ShapeDrawMode
    params: StimulusParams | None
    id: str = ""
    name: str = ""
    anim_enabled: bool = (
        True  # animation-level enable (False when animation holds it off)
    )

    @classmethod
    def from_proto(cls, proto: query_pb2.QueryStimulusResponse) -> StimulusInfo:
        shape_which = (
            proto.params.WhichOneof("shape") if proto.HasField("params") else None
        )
        if shape_which == "rect":
            params: StimulusParams | None = RectParams(
                width=proto.params.rect.width,
                height=proto.params.rect.height,
            )
        elif shape_which == "circle":
            params = CircleParams(radius=proto.params.circle.radius)
        elif shape_which == "ellipse":
            params = EllipseParams(
                width=proto.params.ellipse.width,
                height=proto.params.ellipse.height,
            )
        elif shape_which == "grating":
            params = GratingParams.from_proto(proto.params.grating)
        else:
            params = None

        return cls(
            stimulus_type=_STIMULUS_TYPE_MAP.get(
                proto.stimulus_type, StimulusType.UNKNOWN
            ),
            enabled=proto.enabled,
            pos=Vec2.from_proto(proto.pos) if proto.HasField("pos") else Vec2(0.0, 0.0),
            orientation=proto.orientation,
            opacity=proto.opacity,
            fill_color=Color.from_proto(proto.fill_color)
            if proto.HasField("fill_color")
            else Color(0.0, 0.0, 0.0),
            outline_color=Color.from_proto(proto.outline_color)
            if proto.HasField("outline_color")
            else Color(0.0, 0.0, 0.0),
            outline_width=proto.outline_width,
            draw_mode=_PROTO_TO_DRAW_MODE.get(proto.draw_mode, ShapeDrawMode.FILLED),
            params=params,
            id=proto.id,
            name=proto.name,
            anim_enabled=proto.anim_enabled,
        )
