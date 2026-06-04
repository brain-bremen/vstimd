from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Union

from vstimd._proto.vstimd.v1 import vec2_pb2, color_pb2
from vstimd._proto.vstimd.v1.stimuli import query_pb2, shapes_pb2, stimulus_type_pb2
from .grating_models import GratingMask, GratingParams, GratingTexture


class StimulusType(Enum):
    UNKNOWN  = "unknown"
    RECT     = "rect"
    CIRCLE   = "circle"
    ELLIPSE  = "ellipse"
    BITMAP   = "bitmap"
    SHADER   = "shader"
    PARTICLE = "particle"
    GRATING  = "grating"
    TEXT     = "text"
    POLYGON  = "polygon"


class DrawMode(Enum):
    FILLED             = "filled"
    OUTLINED           = "outlined"
    FILLED_AND_OUTLINED = "filled_and_outlined"


@dataclass
class Color:
    r: float
    g: float
    b: float
    a: float = 1.0

    @classmethod
    def from_proto(cls, proto: color_pb2.Color) -> Color:
        return cls(r=proto.r, g=proto.g, b=proto.b, a=proto.a)


@dataclass
class Vec2:
    x: float
    y: float

    @classmethod
    def from_proto(cls, proto: vec2_pb2.Vec2) -> Vec2:
        return cls(x=proto.x, y=proto.y)


@dataclass
class RectParams:
    width: float
    height: float


@dataclass
class CircleParams:
    radius: float


@dataclass
class EllipseParams:
    width: float
    height: float


StimulusParams = Union[RectParams, CircleParams, EllipseParams, GratingParams]

_STIMULUS_TYPE_MAP: dict[int, StimulusType] = {
    stimulus_type_pb2.STIMULUS_TYPE_RECT:     StimulusType.RECT,
    stimulus_type_pb2.STIMULUS_TYPE_CIRCLE:   StimulusType.CIRCLE,
    stimulus_type_pb2.STIMULUS_TYPE_ELLIPSE:  StimulusType.ELLIPSE,
    stimulus_type_pb2.STIMULUS_TYPE_BITMAP:   StimulusType.BITMAP,
    stimulus_type_pb2.STIMULUS_TYPE_SHADER:   StimulusType.SHADER,
    stimulus_type_pb2.STIMULUS_TYPE_PARTICLE: StimulusType.PARTICLE,
    stimulus_type_pb2.STIMULUS_TYPE_GRATING:  StimulusType.GRATING,
    stimulus_type_pb2.STIMULUS_TYPE_TEXT:     StimulusType.TEXT,
    stimulus_type_pb2.STIMULUS_TYPE_POLYGON:  StimulusType.POLYGON,
}

_DRAW_MODE_MAP: dict[int, DrawMode] = {
    shapes_pb2.SHAPE_DRAW_MODE_FILLED:              DrawMode.FILLED,
    shapes_pb2.SHAPE_DRAW_MODE_OUTLINED:            DrawMode.OUTLINED,
    shapes_pb2.SHAPE_DRAW_MODE_FILLED_AND_OUTLINED: DrawMode.FILLED_AND_OUTLINED,
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
    draw_mode: DrawMode
    params: StimulusParams | None
    id: str = ""
    name: str = ""

    @classmethod
    def from_proto(cls, proto: query_pb2.QueryStimulusResponse) -> StimulusInfo:
        shape_which = proto.params.WhichOneof("shape") if proto.HasField("params") else None
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
            stimulus_type=_STIMULUS_TYPE_MAP.get(proto.stimulus_type, StimulusType.UNKNOWN),
            enabled=proto.enabled,
            pos=Vec2.from_proto(proto.pos) if proto.HasField("pos") else Vec2(0.0, 0.0),
            orientation=proto.orientation,
            opacity=proto.opacity,
            fill_color=Color.from_proto(proto.fill_color) if proto.HasField("fill_color") else Color(0.0, 0.0, 0.0),
            outline_color=Color.from_proto(proto.outline_color) if proto.HasField("outline_color") else Color(0.0, 0.0, 0.0),
            outline_width=proto.outline_width,
            draw_mode=_DRAW_MODE_MAP.get(proto.draw_mode, DrawMode.FILLED),
            params=params,
            id=proto.id,
            name=proto.name,
        )
