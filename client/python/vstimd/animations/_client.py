from __future__ import annotations

import math
from typing import Callable, Optional, Union

from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import animations_pb2, vtl_pb2
from ._models import AnimationInfo, AnimationState, FinalAction, VtlEdge


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
    animations (e.g. ``TriggerFlash``) wait for an edge after arming; free-running
    animations (e.g. ``Flash``) start immediately when armed.

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

    def _to_frames(
        self,
        frames: int | None,
        ms: float | None,
        param: str,
    ) -> int:
        """Convert frames/ms to a frame count; exactly one must be provided."""
        if frames is not None and ms is not None:
            raise ValueError(f"specify either {param}_frames or {param}_ms, not both")
        if frames is not None:
            return frames
        if ms is not None:
            return max(1, math.ceil(ms / 1000.0 * self.fps))
        raise ValueError(f"one of {param}_frames or {param}_ms must be specified")

    def _to_optional_frames(
        self,
        frames: int | None,
        ms: float | None,
        param: str,
    ) -> int:
        """Like _to_frames but None/None → 0 (proto convention: 0 = run forever)."""
        if frames is not None and ms is not None:
            raise ValueError(f"specify either {param}_frames or {param}_ms, not both")
        if frames is not None:
            return frames
        if ms is not None:
            return max(1, math.ceil(ms / 1000.0 * self.fps))
        return 0  # 0 = run forever

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def arm(self, handle: int) -> None:
        """Arm an animation (IDLE → ARMED or RUNNING for free-running types)."""
        req = service_pb2.Request(
            system=_sys(),
            arm_animation=animations_pb2.ArmAnimationRequest(handle=handle),
        )
        self._send(req)

    def disarm(self, handle: int) -> None:
        """Disarm an animation (returns it to IDLE)."""
        req = service_pb2.Request(
            system=_sys(),
            disarm_animation=animations_pb2.DisarmAnimationRequest(handle=handle),
        )
        self._send(req)

    def delete(self, handle: int) -> None:
        """Delete an animation."""
        req = service_pb2.Request(
            system=_sys(),
            delete_animation=animations_pb2.DeleteAnimationRequest(handle=handle),
        )
        self._send(req)

    def list(self) -> list[AnimationInfo]:
        """Return all animations and their current state."""
        req = service_pb2.Request(
            system=_sys(),
            list_animations=animations_pb2.ListAnimationsRequest(),
        )
        resp = self._send(req)
        return [
            AnimationInfo(
                handle=a.handle,
                name=a.name,
                state=AnimationState(a.state),
                type_name=a.type_name,
            )
            for a in resp.animation_list.animations
        ]

    # ── Trigger-reactive animations ───────────────────────────────────────────

    def create_couple_visibility_to_input_trigger_line(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        *,
        polarity: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Mirror stimulus enabled state to the level of an input trigger line."""
        body = animations_pb2.AnimCoupleVisibilityToInputTriggerLine(
            trigger=_make_vtl_handle(trigger),
            polarity=polarity,
            stimuli=_to_stimuli(stimuli),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            couple_visibility_to_input_trigger_line=body,
        ))

    def create_edge_set_enabled(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        *,
        edge: VtlEdge = VtlEdge.RISING,
        enabled: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Set stimulus enabled once when a trigger edge fires."""
        body = animations_pb2.AnimEdgeSetEnabled(
            trigger=_make_vtl_handle(trigger),
            edge=int(edge),
            stimuli=_to_stimuli(stimuli),
            enabled=enabled,
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            edge_set_enabled=body,
        ))

    def create_flash(
        self,
        stimuli: Stimuli,
        duration_frames: int | None = None,
        *,
        duration_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Enable stimuli for the given duration.

        If ``start_trigger`` is given, waits for that edge before starting;
        otherwise starts immediately when armed.
        """
        body = animations_pb2.AnimFlash(
            stimuli=_to_stimuli(stimuli),
            duration_frames=self._to_frames(duration_frames, duration_ms, "duration"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            flash=body,
        ))

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
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Flicker stimuli on/off. Omit ``total_*`` to run forever.

        If ``start_trigger`` is given, waits for that edge before starting;
        otherwise starts immediately when armed.
        """
        body = animations_pb2.AnimFlicker(
            stimuli=_to_stimuli(stimuli),
            on_frames=self._to_frames(on_frames, on_ms, "on"),
            off_frames=self._to_frames(off_frames, off_ms, "off"),
            total_frames=self._to_optional_frames(total_frames, total_ms, "total"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            flicker=body,
        ))

    def create_external_position_2d(
        self,
        stimuli: Stimuli,
        shm_name: str,
        *,
        x_offset: float = 0.0,
        y_offset: float = 0.0,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
        start_trigger: Optional[VtlHandle] = None,
        start_edge: VtlEdge = VtlEdge.RISING,
    ) -> int:
        """Read stimulus position from a POSIX shared memory float array each frame."""
        body = animations_pb2.AnimExternalPosition2D(
            stimuli=_to_stimuli(stimuli),
            shm_name=shm_name,
            x_offset=x_offset,
            y_offset=y_offset,
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            start_trigger=_make_vtl_handle(start_trigger) if start_trigger else None,
            start_edge=int(start_edge),
            external_position_2d=body,
        ))

    # ── Internal ──────────────────────────────────────────────────────────────

    def _create(self, proto_req) -> int:
        req = service_pb2.Request(system=_sys(), create_animation=proto_req)
        resp = self._send(req)
        return resp.handle
