from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum

from vstimd._proto import stimuli_2d_pb2 as stimuli_pb2


class GratingTexture(StrEnum):
    SIN = "sin"
    SQR = "sqr"
    SAW = "saw"
    TRI = "tri"


class GratingMask(StrEnum):
    NONE       = "none"
    CIRCLE     = "circle"
    GAUSS      = "gauss"
    RAISED_COS = "raisedCos"
    HANN       = "hann"


_WAVEFORM_TO_PROTO: dict[GratingTexture, int] = {
    GratingTexture.SIN: stimuli_pb2.WAVEFORM_TYPE_SIN,
    GratingTexture.SQR: stimuli_pb2.WAVEFORM_TYPE_SQR,
    GratingTexture.SAW: stimuli_pb2.WAVEFORM_TYPE_SAW,
    GratingTexture.TRI: stimuli_pb2.WAVEFORM_TYPE_TRI,
}

_PROTO_TO_WAVEFORM: dict[int, GratingTexture] = {v: k for k, v in _WAVEFORM_TO_PROTO.items()}

_MASK_TO_PROTO: dict[GratingMask, int] = {
    GratingMask.NONE:       stimuli_pb2.MASK_TYPE_NONE,
    GratingMask.CIRCLE:     stimuli_pb2.MASK_TYPE_CIRCLE,
    GratingMask.GAUSS:      stimuli_pb2.MASK_TYPE_GAUSS,
    GratingMask.RAISED_COS: stimuli_pb2.MASK_TYPE_RAISED_COS,
    GratingMask.HANN:       stimuli_pb2.MASK_TYPE_HANN,
}

_PROTO_TO_MASK: dict[int, GratingMask] = {v: k for k, v in _MASK_TO_PROTO.items()}


@dataclass
class GratingParams:
    width: float
    height: float
    sf: float
    phase: float
    contrast: float
    waveform: GratingTexture
    mask: GratingMask
    mask_param: float
    drift_speed: float
    drift_coupled: bool
    drift_angle: float
    fore_color: tuple[float, float, float, float] = (1.0, 1.0, 1.0, 1.0)
    back_color: tuple[float, float, float, float] = (0.0, 0.0, 0.0, 1.0)
    opacity: float = 1.0

    @classmethod
    def from_proto(cls, proto: stimuli_pb2.GratingParams) -> GratingParams:
        fore = (1.0, 1.0, 1.0, 1.0)
        back = (0.0, 0.0, 0.0, 1.0)
        if proto.HasField("fore_color"):
            c = proto.fore_color
            fore = (c.r, c.g, c.b, c.a)
        if proto.HasField("back_color"):
            c = proto.back_color
            back = (c.r, c.g, c.b, c.a)
        return cls(
            width=proto.width,
            height=proto.height,
            sf=proto.sf,
            phase=proto.phase,
            contrast=proto.contrast,
            waveform=_PROTO_TO_WAVEFORM.get(proto.waveform, GratingTexture.SIN),
            mask=_PROTO_TO_MASK.get(proto.mask, GratingMask.NONE),
            mask_param=proto.mask_param,
            drift_speed=proto.drift_speed,
            drift_coupled=not proto.drift_decoupled,
            drift_angle=proto.drift_angle,
            fore_color=fore,
            back_color=back,
            opacity=proto.opacity if proto.opacity != 0.0 else 1.0,
        )
