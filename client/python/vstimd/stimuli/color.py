from dataclasses import dataclass
from typing import Self

from vstimd._proto.vstimd.v1 import color_pb2


@dataclass
class Color:
    r: float
    g: float
    b: float
    a: float = 1.0

    @classmethod
    def from_proto(cls, proto: color_pb2.Color) -> Self:
        return cls(r=proto.r, g=proto.g, b=proto.b, a=proto.a)
