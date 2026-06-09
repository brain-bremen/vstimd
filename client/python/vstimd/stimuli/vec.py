from dataclasses import dataclass
from typing import Self

from vstimd._proto.vstimd.v1 import vec2_pb2


@dataclass
class Vec2:
    x: float
    y: float

    @classmethod
    def from_proto(cls, proto: vec2_pb2.Vec2) -> Self:
        return cls(x=proto.x, y=proto.y)


@dataclass
class Vec3:
    x: float
    y: float
    z: float

    # @classmethod
    # def from_proto(cls, proto: vec2_pb2.Vec3) -> Self:
    #     return cls(x=proto.x, y=proto.y, z=proto.z)
