from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import vec2_pb2, color_pb2
from vstimd._proto.vstimd.v1.stimuli import grating_pb2

from .color import Color
from .grating_models import (
    GratingMask,
    GratingParams,
    GratingTexture,
    _MASK_TO_PROTO,
    _WAVEFORM_TO_PROTO,
)
from .vec import Vec2

_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class GratingClient:
    """Create and mutate grating stimuli."""

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    # ── Creation ──────────────────────────────────────────────────────────────

    def create_grating(
        self,
        *,
        pos: Vec2 = Vec2(0.0, 0.0),
        width: float = 200.0,
        height: float = 200.0,
        sf: float = 0.05,
        phase: float = 0.0,
        angle: float = 0.0,
        contrast: float = 1.0,
        fore_color: Color = Color(1.0, 1.0, 1.0),
        back_color: Color = Color(0.0, 0.0, 0.0),
        opacity: float = 1.0,
        waveform: GratingTexture = GratingTexture.SIN,
        mask: GratingMask = GratingMask.NONE,
        mask_param: float = 0.0,
        drift_speed: float = 0.0,
        drift_decoupled: bool = False,
        drift_angle: float = 0.0,
        name: str = "",
        id: str = "",
    ) -> StimulusHandle:
        """Create a grating stimulus and return its handle.

        The grating interpolates between back_color (carrier = -1) and fore_color
        (carrier = +1), modulated by contrast.  opacity sets global transparency.

        mask_param interpretation (0 = use default):
          - MASK_TYPE_GAUSS:      SD in normalized units where patch radius = 1 (default 1/3)
          - MASK_TYPE_RAISED_COS: fringe proportion [0, 1] (default 0.2)
        """
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_grating=grating_pb2.CreateGratingRequest(
                center=vec2_pb2.Vec2(x=pos.x, y=pos.y),
                width=width,
                height=height,
                sf=sf,
                phase=phase,
                angle=angle,
                contrast=contrast,
                fore_color=color_pb2.Color(r=fore_color.r, g=fore_color.g, b=fore_color.b, a=fore_color.a),
                back_color=color_pb2.Color(r=back_color.r, g=back_color.g, b=back_color.b, a=back_color.a),
                opacity=opacity,
                waveform=_WAVEFORM_TO_PROTO[waveform],
                mask=_MASK_TO_PROTO[mask],
                mask_param=mask_param,
                drift_speed=drift_speed,
                drift_decoupled=drift_decoupled,
                drift_angle=drift_angle,
                name=name,
                id=id,
            ),
        )
        return StimulusHandle(self._send(req).handle)

    # ── Grating-specific mutations ─────────────────────────────────────────────

    def set_phase(self, handle: StimulusHandle, phase: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_phase=grating_pb2.SetGratingPhaseRequest(phase=phase),
        ))

    def set_sf(self, handle: StimulusHandle, sf: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_sf=grating_pb2.SetGratingSfRequest(sf=sf),
        ))

    def set_contrast(self, handle: StimulusHandle, contrast: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_contrast=grating_pb2.SetGratingContrastRequest(contrast=contrast),
        ))

    def set_waveform(self, handle: StimulusHandle, waveform: GratingTexture) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_waveform=grating_pb2.SetGratingWaveformRequest(
                waveform=_WAVEFORM_TO_PROTO[waveform],
            ),
        ))

    def set_mask(self, handle: StimulusHandle, mask: GratingMask) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_mask=grating_pb2.SetGratingMaskRequest(mask=_MASK_TO_PROTO[mask]),
        ))

    def set_drift_speed(self, handle: StimulusHandle, drift_speed: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_drift_speed=grating_pb2.SetGratingDriftSpeedRequest(speed=drift_speed),
        ))

    def set_drift_decoupled(self, handle: StimulusHandle, drift_decoupled: bool) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_drift_decoupled=grating_pb2.SetGratingDriftDecoupledRequest(
                decoupled=drift_decoupled,
            ),
        ))

    def set_drift_angle(self, handle: StimulusHandle, drift_angle: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_drift_angle=grating_pb2.SetGratingDriftAngleRequest(angle_deg=drift_angle),
        ))

    def set_fore_color(self, handle: StimulusHandle, color: Color) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_fore_color=grating_pb2.SetGratingForeColorRequest(
                fore_color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
            ),
        ))

    def set_back_color(self, handle: StimulusHandle, color: Color) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_back_color=grating_pb2.SetGratingBackColorRequest(
                back_color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
            ),
        ))

    def set_opacity(self, handle: StimulusHandle, opacity: float) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_grating_opacity=grating_pb2.SetGratingOpacityRequest(opacity=opacity),
        ))
