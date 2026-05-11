from __future__ import annotations

from typing import Callable

from wonderlamp._proto import common_pb2, service_pb2, stimuli_2d_pb2 as stimuli_pb2
from ._models import StimulusInfo


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
    ) -> int:
        """Create a rectangle stimulus and return its handle."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_rect=stimuli_pb2.CreateRectRequest(
                center=common_pb2.Vec2(x=x, y=y),
                width=width,
                height=height,
                fill=common_pb2.Color(r=r, g=g, b=b, a=a),
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
    ) -> int:
        """Create a disc stimulus and return its handle."""
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_circle=stimuli_pb2.CreateCircleRequest(
                center=common_pb2.Vec2(x=x, y=y),
                radius=radius,
                fill=common_pb2.Color(r=r, g=g, b=b, a=a),
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
            ),
        )
        return self._send(req).handle

    # ── Lifecycle ─────────────────────────────────────────────────────────────

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

    def set_disc_radius(self, handle: int, radius: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_disc_radius=stimuli_pb2.SetDiscRadiusRequest(radius=radius),
        )
        self._send(req)

    def set_ellipse_size(self, handle: int, width: float, height: float) -> None:
        req = service_pb2.Request(
            stimulus=handle,
            set_ellipse_size=stimuli_pb2.SetEllipseSizeRequest(width=width, height=height),
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
