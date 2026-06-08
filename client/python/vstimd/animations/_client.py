from __future__ import annotations

import math
from typing import Callable, Optional, Union

from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import animations_pb2, vtl_pb2
from ._models import AnimatedParam, AnimationInfo, AnimationState, FinalAction, VtlEdge


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

    def create_couple_visibility(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        *,
        polarity: bool = True,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Mirror stimulus enabled state to the level of a trigger line."""
        body = animations_pb2.AnimCoupleVisibility(
            trigger=_make_vtl_handle(trigger),
            polarity=polarity,
            stimuli=_to_stimuli(stimuli),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            couple_visibility=body,
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
            edge_set_enabled=body,
        ))

    def create_trigger_flash(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        duration_frames: int | None = None,
        *,
        duration_ms: float | None = None,
        edge: VtlEdge = VtlEdge.RISING,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Enable a stimulus for the given duration after a trigger edge."""
        body = animations_pb2.AnimTriggerFlash(
            trigger=_make_vtl_handle(trigger),
            edge=int(edge),
            stimuli=_to_stimuli(stimuli),
            duration_frames=self._to_frames(duration_frames, duration_ms, "duration"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            trigger_flash=body,
        ))

    def create_trigger_flicker(
        self,
        trigger: VtlHandle,
        stimuli: Stimuli,
        on_frames: int | None = None,
        off_frames: int | None = None,
        *,
        on_ms: float | None = None,
        off_ms: float | None = None,
        edge: VtlEdge = VtlEdge.RISING,
        total_frames: int | None = None,
        total_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Flicker a stimulus on/off after a trigger edge. Omit ``total_*`` to run forever."""
        body = animations_pb2.AnimTriggerFlicker(
            trigger=_make_vtl_handle(trigger),
            edge=int(edge),
            stimuli=_to_stimuli(stimuli),
            on_frames=self._to_frames(on_frames, on_ms, "on"),
            off_frames=self._to_frames(off_frames, off_ms, "off"),
            total_frames=self._to_optional_frames(total_frames, total_ms, "total"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            trigger_flicker=body,
        ))

    # ── Free-running animations ───────────────────────────────────────────────

    def create_flash(
        self,
        stimuli: Stimuli,
        duration_frames: int | None = None,
        *,
        duration_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Enable a stimulus for the given duration immediately when armed."""
        body = animations_pb2.AnimFlash(
            stimuli=_to_stimuli(stimuli),
            duration_frames=self._to_frames(duration_frames, duration_ms, "duration"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
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
    ) -> int:
        """Flicker a stimulus on/off immediately when armed. Omit ``total_*`` to run forever."""
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
            flicker=body,
        ))

    def create_harmonic(
        self,
        stimuli: Stimuli,
        amplitude: float,
        phase_inc: float,
        *,
        direction_deg: float = 0.0,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Sinusoidal position oscillation. ``phase_inc`` is radians per frame."""
        body = animations_pb2.AnimHarmonic(
            stimuli=_to_stimuli(stimuli),
            amplitude=amplitude,
            phase_inc=phase_inc,
            direction_deg=direction_deg,
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            harmonic=body,
        ))

    def create_linear_range(
        self,
        stimuli: Stimuli,
        param: AnimatedParam,
        start: float,
        end: float,
        duration_frames: int | None = None,
        *,
        duration_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Linearly interpolate a parameter over the given duration."""
        body = animations_pb2.AnimLinearRange(
            stimuli=_to_stimuli(stimuli),
            param=int(param),
            start=start,
            end=end,
            duration_frames=self._to_frames(duration_frames, duration_ms, "duration"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            linear_range=body,
        ))

    def create_external_position(
        self,
        stimuli: Stimuli,
        shm_name: str,
        *,
        x_offset: float = 0.0,
        y_offset: float = 0.0,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Read stimulus position from a POSIX shared memory float array each frame."""
        body = animations_pb2.AnimExternalPosition(
            stimuli=_to_stimuli(stimuli),
            shm_name=shm_name,
            x_offset=x_offset,
            y_offset=y_offset,
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            external_position=body,
        ))

    # ── Output-driving animations ─────────────────────────────────────────────

    def create_frame_onset_output(
        self,
        output: VtlHandle,
        pulse_frames: int | None = None,
        *,
        pulse_ms: float | None = None,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Drive an output line HIGH for the given duration each frame."""
        body = animations_pb2.AnimFrameOnsetOutput(
            output=_make_vtl_handle(output),
            pulse_frames=self._to_frames(pulse_frames, pulse_ms, "pulse"),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            frame_onset_output=body,
        ))

    def create_stimulus_visible_out(
        self,
        output: VtlHandle,
        stimuli: Stimuli,
        *,
        name: str = "",
        final_action_mask: FinalAction = FinalAction(0),
        signal_event_output: Optional[VtlHandle] = None,
    ) -> int:
        """Mirror stimulus visibility to an output line."""
        body = animations_pb2.AnimStimulusVisibleOut(
            output=_make_vtl_handle(output),
            stimuli=_to_stimuli(stimuli),
        )
        return self._create(animations_pb2.CreateAnimationRequest(
            name=name,
            final_action_mask=int(final_action_mask),
            signal_event_output=_make_vtl_handle(signal_event_output) if signal_event_output else None,
            stimulus_visible_out=body,
        ))

    # ── Internal ──────────────────────────────────────────────────────────────

    def _create(self, proto_req) -> int:
        req = service_pb2.Request(system=_sys(), create_animation=proto_req)
        resp = self._send(req)
        return resp.handle
