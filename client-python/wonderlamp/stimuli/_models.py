from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Union

from wonderlamp._proto import common_pb2, stimuli_2d_pb2 as stimuli_pb2


class StimulusType(Enum):
    UNKNOWN  = "unknown"
    RECT     = "rect"
    DISC     = "disc"
    ELLIPSE  = "ellipse"
    BITMAP   = "bitmap"
    SHADER   = "shader"
    PARTICLE = "particle"


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
    def from_proto(cls, proto: common_pb2.Color) -> Color:
        return cls(r=proto.r, g=proto.g, b=proto.b, a=proto.a)


@dataclass
class Vec2:
    x: float
    y: float

    @classmethod
    def from_proto(cls, proto: common_pb2.Vec2) -> Vec2:
        return cls(x=proto.x, y=proto.y)


@dataclass
class RectParams:
    width: float
    height: float


@dataclass
class DiscParams:
    radius: float


@dataclass
class EllipseParams:
    width: float
    height: float


StimulusParams = Union[RectParams, DiscParams, EllipseParams]

_STIMULUS_TYPE_MAP: dict[int, StimulusType] = {
    common_pb2.STIMULUS_TYPE_RECT:     StimulusType.RECT,
    common_pb2.STIMULUS_TYPE_DISC:     StimulusType.DISC,
    common_pb2.STIMULUS_TYPE_ELLIPSE:  StimulusType.ELLIPSE,
    common_pb2.STIMULUS_TYPE_BITMAP:   StimulusType.BITMAP,
    common_pb2.STIMULUS_TYPE_SHADER:   StimulusType.SHADER,
    common_pb2.STIMULUS_TYPE_PARTICLE: StimulusType.PARTICLE,
}

_DRAW_MODE_MAP: dict[int, DrawMode] = {
    common_pb2.DRAW_MODE_FILLED:              DrawMode.FILLED,
    common_pb2.DRAW_MODE_OUTLINED:            DrawMode.OUTLINED,
    common_pb2.DRAW_MODE_FILLED_AND_OUTLINED: DrawMode.FILLED_AND_OUTLINED,
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

    @classmethod
    def from_proto(cls, proto: stimuli_pb2.QueryStimulusResponse) -> StimulusInfo:
        shape_which = proto.params.WhichOneof("shape") if proto.HasField("params") else None
        if shape_which == "rect":
            params: StimulusParams | None = RectParams(
                width=proto.params.rect.width,
                height=proto.params.rect.height,
            )
        elif shape_which == "disc":
            params = DiscParams(radius=proto.params.disc.radius)
        elif shape_which == "ellipse":
            params = EllipseParams(
                width=proto.params.ellipse.width,
                height=proto.params.ellipse.height,
            )
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
        )
