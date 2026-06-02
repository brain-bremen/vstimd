from __future__ import annotations

from typing import Callable

from vstimd._proto import common_pb2, service_pb2, stimuli_2d_pb2 as stimuli_pb2
from .stimuli_models import Color, StimulusInfo, Vec2
from .grating_models import GratingMask, GratingTexture, _MASK_TO_PROTO, _WAVEFORM_TO_PROTO


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class StimuliClient:
    """Commands addressed to individual stimuli (create, mutate, query)."""

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    # ── Creation ──────────────────────────────────────────────────────────────

    def create_rect(
        self,
        *,
        x: float = 0.0,
        y: float = 0.0,
        width: float = 100.0,
        height: float = 100.0,
        r: float = 1.0,
        g: float = 1.0,
        b: float = 1.0,
        a: float = 1.0,
        name: str = "",
        id: str = "",
    ) -> int:
        """Create a rectangle stimulus and return its handle."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_rect=stimuli_pb2.CreateRectRequest(
                center=common_pb2.Vec2(x=x, y=y),
                width=width,
                height=height,
                fill=common_pb2.Color(r=r, g=g, b=b, a=a),
                name=name,
                id=id,
            ),
        )
        return self._send(req).handle

    def create_circle(
        self,
        *,
        x: float = 0.0,
        y: float = 0.0,
        radius: float = 50.0,
        r: float = 1.0,
        g: float = 1.0,
        b: float = 1.0,
        a: float = 1.0,
        name: str = "",
        id: str = "",
    ) -> int:
        """Create a circle stimulus and return its handle."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_circle=stimuli_pb2.CreateCircleRequest(
                center=common_pb2.Vec2(x=x, y=y),
                radius=radius,
                fill=common_pb2.Color(r=r, g=g, b=b, a=a),
                name=name,
                id=id,
            ),
        )
        return self._send(req).handle

    def create_ellipse(
        self,
        *,
        x: float = 0.0,
        y: float = 0.0,
        width: float = 100.0,
        height: float = 50.0,
        angle: float = 0.0,
        r: float = 1.0,
        g: float = 1.0,
        b: float = 1.0,
        a: float = 1.0,
        name: str = "",
        id: str = "",
    ) -> int:
        """Create an ellipse stimulus and return its handle."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_ellipse=stimuli_pb2.CreateEllipseRequest(
                center=common_pb2.Vec2(x=x, y=y),
                width=width,
                height=height,
                angle=angle,
                fill=common_pb2.Color(r=r, g=g, b=b, a=a),
                name=name,
                id=id,
            ),
        )
        return self._send(req).handle

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
    ) -> int:
        """Create a grating stimulus and return its handle.

        The grating interpolates between back_color (carrier = -1) and fore_color
        (carrier = +1), modulated by contrast.  opacity sets global transparency.

        mask_param interpretation (0 = use default):
          - MASK_TYPE_GAUSS:      SD in normalized units where patch radius = 1 (default 1/3)
          - MASK_TYPE_RAISED_COS: fringe proportion [0, 1] (default 0.2)
        """
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_grating=stimuli_pb2.CreateGratingRequest(
                center=common_pb2.Vec2(x=pos.x, y=pos.y),
                width=width,
                height=height,
                sf=sf,
                phase=phase,
                angle=angle,
                contrast=contrast,
                fore_color=common_pb2.Color(r=fore_color.r, g=fore_color.g, b=fore_color.b, a=fore_color.a),
                back_color=common_pb2.Color(r=back_color.r, g=back_color.g, b=back_color.b, a=back_color.a),
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
        return self._send(req).handle

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def set_name(self, handle: int, name: str) -> None:
        """Rename a stimulus (does not affect handle or UUID)."""
        req = service_pb2.Request(
            stimulus=handle,
            set_name=stimuli_pb2.SetNameRequest(name=name),
        )
        self._send(req)

    def set_enabled(self, handle: int, enabled: bool) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_enabled=stimuli_pb2.SetEnabledRequest(enabled=enabled),
        )
        self._send(req)

    def delete(self, handle: int) -> None:
        req = service_pb2.Request(stimulus=handle, delete=stimuli_pb2.DeleteRequest())
        self._send(req)

    # ── Transform ─────────────────────────────────────────────────────────────

    def set_position(self, handle: int, x: float, y: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_position=stimuli_pb2.SetPositionRequest(x=x, y=y),
        )
        self._send(req)

    def set_orientation(self, handle: int, angle_deg: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_orientation=stimuli_pb2.SetOrientationRequest(angle_deg=angle_deg),
        )
        self._send(req)

    # ── Appearance ────────────────────────────────────────────────────────────

    def set_fill_color(self, handle: int, r: float, g: float, b: float, a: float = 1.0) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_fill_color=stimuli_pb2.SetFillColorRequest(color=common_pb2.Color(r=r, g=g, b=b, a=a)),
        )
        self._send(req)

    def set_alpha(self, handle: int, opacity: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_alpha=stimuli_pb2.SetAlphaRequest(opacity=opacity),
        )
        self._send(req)

    def set_outline_color(self, handle: int, r: float, g: float, b: float, a: float = 1.0) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_outline_color=stimuli_pb2.SetOutlineColorRequest(
                color=common_pb2.Color(r=r, g=g, b=b, a=a)
            ),
        )
        self._send(req)

    def set_outline_width(self, handle: int, line_width: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_outline_width=stimuli_pb2.SetOutlineWidthRequest(line_width=line_width),
        )
        self._send(req)

    # ── Shape-specific ────────────────────────────────────────────────────────

    def set_rect_size(self, handle: int, width: float, height: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_rect_size=stimuli_pb2.SetRectSizeRequest(width=width, height=height),
        )
        self._send(req)

    def set_circle_radius(self, handle: int, radius: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_circle_radius=stimuli_pb2.SetCircleRadiusRequest(radius=radius),
        )
        self._send(req)

    def set_ellipse_size(self, handle: int, width: float, height: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_ellipse_size=stimuli_pb2.SetEllipseSizeRequest(width=width, height=height),
        )
        self._send(req)

    def set_grating_phase(self, handle: int, phase: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_phase=stimuli_pb2.SetGratingPhaseRequest(phase=phase),
        )
        self._send(req)

    def set_grating_sf(self, handle: int, sf: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_sf=stimuli_pb2.SetGratingSfRequest(sf=sf),
        )
        self._send(req)

    def set_grating_contrast(self, handle: int, contrast: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_contrast=stimuli_pb2.SetGratingContrastRequest(contrast=contrast),
        )
        self._send(req)

    def set_grating_waveform(self, handle: int, waveform: GratingTexture) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_waveform=stimuli_pb2.SetGratingWaveformRequest(waveform=_WAVEFORM_TO_PROTO[waveform]),
        )
        self._send(req)

    def set_grating_mask(self, handle: int, mask: GratingMask) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_mask=stimuli_pb2.SetGratingMaskRequest(mask=_MASK_TO_PROTO[mask]),
        )
        self._send(req)

    def set_grating_drift_speed(self, handle: int, drift_speed: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_drift_speed=stimuli_pb2.SetGratingDriftSpeedRequest(speed=drift_speed),
        )
        self._send(req)

    def set_grating_drift_decoupled(self, handle: int, drift_decoupled: bool) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_drift_decoupled=stimuli_pb2.SetGratingDriftDecoupledRequest(
                decoupled=drift_decoupled
            ),
        )
        self._send(req)

    def set_grating_drift_angle(self, handle: int, drift_angle: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_drift_angle=stimuli_pb2.SetGratingDriftAngleRequest(angle_deg=drift_angle),
        )
        self._send(req)

    def set_grating_fore_color(self, handle: int, r: float, g: float, b: float, a: float = 1.0) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_fore_color=stimuli_pb2.SetGratingForeColorRequest(
                fore_color=common_pb2.Color(r=r, g=g, b=b, a=a),
            ),
        )
        self._send(req)

    def set_grating_back_color(self, handle: int, r: float, g: float, b: float, a: float = 1.0) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_back_color=stimuli_pb2.SetGratingBackColorRequest(
                back_color=common_pb2.Color(r=r, g=g, b=b, a=a),
            ),
        )
        self._send(req)

    def set_grating_opacity(self, handle: int, opacity: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_grating_opacity=stimuli_pb2.SetGratingOpacityRequest(opacity=opacity),
        )
        self._send(req)

    # ── Query ─────────────────────────────────────────────────────────────────

    def query(self, handle: int) -> StimulusInfo:
        """Return current server-side properties for the given stimulus handle."""
        req = service_pb2.Request(
            stimulus=handle,
            query_stimulus=stimuli_pb2.QueryStimulusRequest(),
        )
        return StimulusInfo.from_proto(self._send(req).stimulus_info)
