from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum

from vstimd._proto.vstimd.v1.stimuli import shapes_pb2


class ShapeDrawMode(StrEnum):
    FILLED = "filled"
    OUTLINED = "outlined"
    FILLED_AND_OUTLINED = "filled_and_outlined"


_PROTO_TO_DRAW_MODE: dict[int, ShapeDrawMode] = {
    shapes_pb2.SHAPE_DRAW_MODE_FILLED: ShapeDrawMode.FILLED,
    shapes_pb2.SHAPE_DRAW_MODE_OUTLINED: ShapeDrawMode.OUTLINED,
    shapes_pb2.SHAPE_DRAW_MODE_FILLED_AND_OUTLINED: ShapeDrawMode.FILLED_AND_OUTLINED,
}

_SHAPE_DRAW_MODE_TO_PROTO: dict[ShapeDrawMode, shapes_pb2.ShapeDrawMode] = {
    ShapeDrawMode.FILLED: shapes_pb2.SHAPE_DRAW_MODE_FILLED,
    ShapeDrawMode.OUTLINED: shapes_pb2.SHAPE_DRAW_MODE_OUTLINED,
    ShapeDrawMode.FILLED_AND_OUTLINED: shapes_pb2.SHAPE_DRAW_MODE_FILLED_AND_OUTLINED,
}


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
