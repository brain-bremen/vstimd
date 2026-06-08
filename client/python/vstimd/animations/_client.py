from __future__ import annotations

import math
from typing import Callable, Optional, Union

from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import animations_pb2, vtl_pb2
from ._models import AnimationDetails, AnimationInfo, AnimationState, FinalAction, VtlEdge


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]
_FpsGetter = Callable[[], float]

# A VTL line can be addressed by (bank, bit) or by registered name.
VtlHandle = Union[tuple[int, int], str]

# A stimulus or list of stimuli.
Stimuli = Union[int, list[int]]


def _to_stimuli(s: Stimuli) -> list[int]:
    return [s] if isinstance(s, int) else list(s)


def _make_vtl_handle(handle: VtlHandle) -> vtl_pb2.VirtualTriggerLineHandle:
    if isinstance(handle, str):
        return vtl_pb2.VirtualTriggerLineHandle(name=handle)
    bank, bit = handle
    return vtl_pb2.VirtualTriggerLineHandle(
        bank_bit=vtl_pb2.VirtualTriggerLineBankBit(bank=bank, bit=bit)
    )


def _sys() -> service_pb2.SystemTarget:
    return service_pb2.SystemTarget()


class AnimationClient:
    """Animation management commands.

    Animations run once per frame in the render loop. They are created in the
    ``IDLE`` state and must be *armed* before they fire.  Trigger-reactive
    animations wait for an edge after arming; free-running animations start
    immediately when armed (unless ``start_trigger`` is also set).

    All ``create_*`` methods return the integer animation handle.

    Frame/time parameters accept either a ``*_frames`` integer or a ``*_ms``
    float.  Specify exactly one; the ms variant is converted using the server's
    reported frame rate, queried lazily on first use and cached.
    """

    def __init__(self, send: _SendFn, fps_getter: _FpsGetter) -> None:
        self._send = send
        self._fps_getter = fps_getter
        self._fps_cache: float | None = None

    @property
    def fps(self) -> float:
        """Server frame rate, queried once on first use."""
        if self._fps_cache is None:
            self._fps_cache = self._fps_getter()
        return self._fps_cache

    def refresh_fps(self) -> None:
        """Invalidate the cached frame rate so it is re-queried on next use."""
        self._fps_cache = None

    # ── Frame/ms conversion helpers ───────────────────────────────────────────

    def _to_frames(self, frames: int | None, ms: float | None, param: str) -> int:
        if frames is not None and ms is not None:
            raise ValueError(f"specify either {param}_frames or {param}_ms, not both")
        if frames is not None:
            return frames
        if ms is not None:
            return max(1, math.ceil(ms / 1000.0 * self.fps))
        raise ValueError(f"one of {param}_frames or {param}_ms must be specified")

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def arm(self, handle: int) -> None:
        """Arm an animation (IDLE → ARMED or RUNNING for free-running types)."""
        self._send(service_pb2.Request(
            system=_sys(),
            arm_animation=animations_pb2.ArmAnimationRequest(handle=handle),
        ))

    def disarm(self, handle: int) -> None:
        """Disarm an animation (returns it to IDLE)."""
        self._send(service_pb2.Request(
            system=_sys(),
            disarm_animation=animations_pb2.DisarmAnimationRequest(handle=handle),
        ))

    def delete(self, handle: int) -> None:
        """Delete an animation."""
        self._send(service_pb2.Request(
            system=_sys(),
            delete_animation=animations_pb2.DeleteAnimationRequest(handle=handle),
        ))

    def list(self) -> list[AnimationInfo]:
        """Return all animations and their current state."""
        resp = self._send(service_pb2.Request(
            system=_sys(),
            list_animations=animations_pb2.ListAnimationsRequest(),
        ))
        return [
            AnimationInfo(
                handle=a.handle,
                name=a.name,
                state=AnimationState(a.state),
                type_name=a.type_name,
            )
            for a in resp.animation_list.animations
        ]

    def query(self, handle: int) -> AnimationDetails:
        """Return the full configuration and current state of an animation."""
        resp = self._send(service_pb2.Request(
            system=_sys(),
            query_animation=animations_pb2.QueryAnimationRequest(handle=handle),
        ))
        r = resp.query_animation_response
        p = r.params
        type_name = p.WhichOneof("body") or "unknown"
        return AnimationDetails(
            handle=r.handle,
            name=p.name,
            state=AnimationState(r.state),
            type_name=type_name,
            stimuli=tuple(p.stimuli),
            final_action=FinalAction(p.final_action_mask),
        )

    # ── Shared keyword args (passed through _make_req) ────────────────────────

    def _make_req(
        self,
        stimuli: Stimuli,
        body_kwargs: dict,
        *,
        name: str,
        final_action_mask: FinalAction,
        final_action_trigger_line: Optional[VtlHandle],
        start_trigger: Optional[VtlHandle],
        start_edge: VtlEdge,
    ) -> animations_pb2.CreateAnimationRequest:
        return animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            final_action_trigger_line=_make_vtl_handle(final_action_trigger_line) if final_action_trigger_line else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            stimuli=_to_stimuli(stimuli),
            **body_kwargs,
        )

    # ── Animation types ───────────────────────────────────────────────────────

    def create_couple_visibility_to_input_trigger_line(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        *,
        polarity: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Mirror stimulus enabled state to the level of an input trigger line."""
        req = self._make_req(
            stimuli, {
                "couple_visibility_to_input_trigger_line":
                    animations_pb2.CoupleVisibilityToInputTriggerLine(
                        trigger=_make_vtl_handle(trigger),
                        polarity=polarity,
                    ),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_enable_on_trigger_edge(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        *,
        edge: VtlEdge = VtlEdge.RISING,
        enabled: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Set stimulus enabled once when a trigger edge fires."""
        req = self._make_req(
            stimuli, {
                "enable_on_trigger_edge": animations_pb2.EnableOnTriggerEdge(
                    trigger=_make_vtl_handle(trigger),
                    edge=int(edge),
                    enabled=enabled,
                ),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_flash(
        self,
        stimuli: Stimuli,
        duration_frames: int | None = None,
        *,
        duration_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Enable stimuli for the given duration.

        If ``start_trigger`` is given, waits for that edge before starting;
        otherwise starts immediately when armed.
        """
        req = self._make_req(
            stimuli, {
                "flash_for_n_frames": animations_pb2.FlashForNFrames(
                    duration_frames=self._to_frames(duration_frames, duration_ms, "duration"),
                ),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_flicker(
        self,
        stimuli: Stimuli,
        on_frames: int | None = None,
        off_frames: int | None = None,
        *,
        on_ms: float | None = None,
        off_ms: float | None = None,
        total_frames: int | None = None,
        total_ms: float | None = None,
        start_on_phase: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Flicker stimuli on/off. Omit ``total_*`` to run forever.

        ``start_on_phase=False`` starts in the off-phase instead of the on-phase.
        If ``start_trigger`` is given, waits for that edge before starting.
        """
        msg = animations_pb2.FlickerForNFrames(
            on_frames=self._to_frames(on_frames, on_ms, "on"),
            off_frames=self._to_frames(off_frames, off_ms, "off"),
            start_on_phase=start_on_phase,
        )
        if total_frames is not None or total_ms is not None:
            msg.total_frames = self._to_frames(total_frames, total_ms, "total")
        req = self._make_req(
            stimuli, {"flicker_for_n_frames": msg},
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_move_along_path_2d(
        self,
        stimuli: Stimuli,
        x: list[float],
        y: list[float],
        *,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Move stimulus through a sequence of 2-D positions, one per frame.

        ``x`` and ``y`` must have the same length. The animation completes after
        all positions have been played. Coordinates are in screen units.
        """
        if len(x) != len(y):
            raise ValueError("x and y must have equal length")
        req = self._make_req(
            stimuli, {
                "move_along_path_2d": animations_pb2.MoveAlongPath2D(x=x, y=y),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_move_along_segments_2d(
        self,
        stimuli: Stimuli,
        x: list[float],
        y: list[float],
        speed_px_per_sec: float,
        *,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Move stimulus along piecewise-linear waypoints at a constant speed.

        ``x`` and ``y`` must have the same length and at least 2 entries.
        ``speed_px_per_sec`` is in screen units per second; the server converts
        to frame steps using the measured display frame rate.
        """
        if len(x) != len(y):
            raise ValueError("x and y must have equal length")
        if len(x) < 2:
            raise ValueError("at least 2 waypoints required")
        req = self._make_req(
            stimuli, {
                "move_along_segments_2d": animations_pb2.MoveAlongSegments2D(
                    x=x, y=y, speed_px_per_sec=speed_px_per_sec,
                ),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    def create_external_position_2d(
        self,
        stimuli: Stimuli,
        shm_name: str,
        *,
        x_offset: float = 0.0,
        y_offset: float = 0.0,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        final_action_trigger_line: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Read stimulus position from a POSIX shared memory float array each frame."""
        req = self._make_req(
            stimuli, {
                "external_position_2d": animations_pb2.ExternalPosition2D(
                    shm_name=shm_name,
                    x_offset=x_offset,
                    y_offset=y_offset,
                ),
            },
            name=name, final_action_mask=final_action_mask,
            final_action_trigger_line=final_action_trigger_line,
            start_trigger=start_trigger, start_edge=start_edge,
        )
        return self._create(req)

    # ── Internal ──────────────────────────────────────────────────────────────

    def _create(self, proto_req: animations_pb2.CreateAnimationRequest) -> int:
        resp = self._send(service_pb2.Request(system=_sys(), create_animation=proto_req))
        return resp.handle
