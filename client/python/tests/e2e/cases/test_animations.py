"""E2E tests for the animation system.

These tests require a server with a running frame loop (real or null renderer).
"""

from __future__ import annotations

import time

import pytest

from vstimd import Connection
from vstimd.animations import AnimationState, FinalAction, StartAction, VtlEdge
from vstimd.stimuli.stimuli_models import Color, Vec2
from vstimd.vtl import VtlDirection

from ._helpers import (
    label as _label,
)
from ._helpers import (
    make_rect as _make_rect,
)
from ._helpers import (
    update_label as _update_label,
)
from ._helpers import (
    wait_for_anim_state as _wait_for_state,
)


def test_anim_flash_state_transitions(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flash runs for N frames and ends in DONE state."""
    tid = request.node.name
    lbl = _label(conn, tid, "flashing red rect — 30 frames")
    s = _make_rect(conn, x=0, y=0, enabled=False)

    a = conn.animations.create_flash(
        s, duration_frames=30, name="flash_30", final_action_mask=FinalAction.DISABLE
    )
    assert conn.animations.query(a).state == AnimationState.IDLE

    # Single source of truth: query() and list_animations() return the same
    # canonical type_name (the serde config tag), sent verbatim by the server.
    assert conn.animations.query(a).type_name == "FlashForNFrames"
    listed = next(i for i in conn.animations.list_animations() if i.handle == a)
    assert listed.type_name == conn.animations.query(a).type_name

    conn.animations.arm(a)
    assert conn.animations.query(a).state in (
        AnimationState.ARMED,
        AnimationState.RUNNING,
    )

    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=4.0)
    assert final == AnimationState.DONE, f"animation did not reach DONE (got {final!r})"

    info = conn.stimuli.query(s)
    assert info.enabled is False, "stimulus should be disabled by DISABLE final action"

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flash_stimulus_visible_during_run(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Stimulus is enabled while flash is running and disabled after DISABLE final action."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect ON during flash, then OFF")
    s = _make_rect(conn, x=-150, y=0, enabled=False)

    a = conn.animations.create_flash(
        s, duration_frames=60, final_action_mask=FinalAction.DISABLE
    )
    conn.animations.arm(a)

    time.sleep(0.1)
    info = conn.stimuli.query(s)
    assert info.enabled is True, "stimulus should be enabled while flash is running"

    _update_label(conn, lbl, tid, "rect ON (flash running)")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=4.0)
    assert final == AnimationState.DONE

    info = conn.stimuli.query(s)
    assert info.enabled is False, "stimulus should be disabled after flash + DISABLE"

    _update_label(conn, lbl, tid, "rect OFF (flash done, DISABLE)")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flash_start_trigger(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flash with start_trigger stays ARMED until a rising edge fires it."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash waiting for trigger edge")
    s = _make_rect(conn, x=150, y=0, enabled=False)

    a = conn.animations.create_flash(
        s,
        duration_frames=30,
        start_trigger=(0, 10),
        start_edge=VtlEdge.RISING,
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(a)

    time.sleep(0.2)
    assert conn.animations.query(a).state == AnimationState.ARMED, (
        "should remain ARMED before trigger"
    )

    _update_label(conn, lbl, tid, "ARMED — waiting for trigger")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 10), True)
    time.sleep(0.1)
    conn.vtl.set_input_line((0, 10), False)

    _update_label(conn, lbl, tid, "triggered — rect ON")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=4.0)
    assert final == AnimationState.DONE

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flash_disarm_resets_state(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Disarming a flash while ARMED returns it to IDLE."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash armed then disarmed")
    s = _make_rect(conn, x=0, y=-100, enabled=False)

    a = conn.animations.create_flash(
        s, duration_frames=120, start_trigger=(0, 11), start_edge=VtlEdge.RISING
    )
    conn.animations.arm(a)

    time.sleep(0.1)
    assert conn.animations.query(a).state == AnimationState.ARMED

    _update_label(conn, lbl, tid, "ARMED, about to disarm")
    time.sleep(step_delay)

    conn.animations.disarm(a)
    assert conn.animations.query(a).state == AnimationState.IDLE

    _update_label(conn, lbl, tid, "IDLE after disarm")
    time.sleep(step_delay * 0.5)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_cancel_command_running(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Cancelling a RUNNING animation via the software command is a clean teardown → DONE.

    Distinct from disarm (which returns to IDLE): cancel runs the final action
    (DISABLE here) and lands in DONE.
    """
    tid = request.node.name
    lbl = _label(conn, tid, "flash cancelled while running (→DONE, disabled)")
    s = _make_rect(conn, x=0, y=0, enabled=False)

    # Long duration so it is still running when we cancel.
    a = conn.animations.create_flash(
        s, duration_frames=600, final_action_mask=FinalAction.DISABLE
    )
    conn.animations.arm(a)

    _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    time.sleep(0.05)
    assert conn.stimuli.query(s).enabled is True, "stimulus enabled while running"

    _update_label(conn, lbl, tid, "RUNNING — about to cancel")
    time.sleep(step_delay)

    conn.animations.cancel(a)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=2.0)
    assert final == AnimationState.DONE, "cancel ends in DONE (not IDLE like disarm)"
    assert conn.stimuli.query(s).enabled is False, "cancel runs DISABLE teardown"

    _update_label(conn, lbl, tid, "cancelled — DONE, rect OFF")
    time.sleep(step_delay * 0.5)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_cancel_trigger_running(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """A cancel_trigger VTL edge aborts a RUNNING animation with clean teardown → DONE."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash cancelled by VTL edge (0,50)")
    s = _make_rect(conn, x=0, y=0, enabled=False)

    a = conn.animations.create_flash(
        s,
        duration_frames=600,
        cancel_trigger=(0, 50),
        cancel_edge=VtlEdge.RISING,
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(a)

    _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    time.sleep(0.05)
    assert conn.stimuli.query(s).enabled is True

    _update_label(conn, lbl, tid, "RUNNING — firing cancel edge")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 50), True)
    time.sleep(0.1)
    conn.vtl.set_input_line((0, 50), False)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=2.0)
    assert final == AnimationState.DONE
    assert conn.stimuli.query(s).enabled is False, "cancel edge ran DISABLE teardown"

    _update_label(conn, lbl, tid, "cancelled by edge — DONE")
    time.sleep(step_delay * 0.5)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_cancel_trigger_while_armed(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """A cancel_trigger edge stops an ARMED animation before it ever starts → DONE."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash cancelled while ARMED (never starts)")
    s = _make_rect(conn, x=0, y=0, enabled=False)

    # Waits on start_trigger (0,51); we never fire it — instead we fire the
    # cancel_trigger (0,52) while it is still ARMED.
    a = conn.animations.create_flash(
        s,
        duration_frames=120,
        start_trigger=(0, 51),
        start_edge=VtlEdge.RISING,
        cancel_trigger=(0, 52),
        cancel_edge=VtlEdge.RISING,
    )
    conn.animations.arm(a)

    time.sleep(0.1)
    assert conn.animations.query(a).state == AnimationState.ARMED

    _update_label(conn, lbl, tid, "ARMED — firing cancel edge")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 52), True)
    time.sleep(0.1)
    conn.vtl.set_input_line((0, 52), False)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=2.0)
    assert final == AnimationState.DONE, "cancelled before start → DONE"
    assert conn.stimuli.query(s).enabled is False, (
        "flash never started; stimulus stays off"
    )

    _update_label(conn, lbl, tid, "cancelled while ARMED — DONE")
    time.sleep(step_delay * 0.5)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flicker_cycles(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flicker toggles a stimulus on and off at the specified cadence."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect flickering 6/6 frames")
    s = _make_rect(conn, x=-200, y=100)

    a = conn.animations.create_flicker(s, on_frames=6, off_frames=6, total_frames=60)
    conn.animations.arm(a)

    _update_label(conn, lbl, tid, "flickering")
    time.sleep(step_delay * 2)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=5.0)
    assert final == AnimationState.DONE, f"flicker did not complete (got {final!r})"

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flicker_indefinite_then_disarm(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Indefinite flicker stays RUNNING until explicitly disarmed."""
    tid = request.node.name
    lbl = _label(conn, tid, "indefinite flicker — then disarmed")
    s = _make_rect(conn, x=200, y=100)

    a = conn.animations.create_flicker(s, on_frames=8, off_frames=8)
    conn.animations.arm(a)

    running = _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    assert running == AnimationState.RUNNING, "indefinite flicker should reach RUNNING"

    _update_label(conn, lbl, tid, "RUNNING (indefinite flicker)")
    time.sleep(step_delay * 2)

    assert conn.animations.query(a).state == AnimationState.RUNNING, (
        "indefinite flicker should stay RUNNING"
    )

    conn.animations.disarm(a)
    assert conn.animations.query(a).state == AnimationState.IDLE

    info = conn.stimuli.query(s)
    assert info.anim_enabled is True, (
        "anim_enabled should be True after disarming flicker"
    )

    _update_label(conn, lbl, tid, "IDLE after disarm")
    time.sleep(step_delay * 0.5)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_flicker_off_phase_start(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flicker with start_on_phase=False begins in the off-phase (stimulus hidden first)."""
    tid = request.node.name
    lbl = _label(conn, tid, "flicker starts in OFF phase")
    s = _make_rect(conn, x=0, y=100)

    # 30 on / 120 off (2 s at 60 fps); starts in off-phase → ample window to observe hidden state
    a = conn.animations.create_flicker(
        s, on_frames=30, off_frames=120, total_frames=150, start_on_phase=False
    )
    conn.animations.arm(a)

    _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    time.sleep(0.05)
    info = conn.stimuli.query(s)
    assert info.anim_enabled is False, (
        "stimulus should start in off-phase (anim_enabled=False)"
    )

    _update_label(conn, lbl, tid, "off-phase (rect hidden)")
    time.sleep(step_delay * 0.5)

    # after the off-phase (120 frames / 60 fps = 2 s) it should flip to on
    time.sleep(2.1)
    info = conn.stimuli.query(s)
    assert info.anim_enabled is True, (
        "stimulus should be in on-phase after off-phase ends"
    )

    _update_label(conn, lbl, tid, "on-phase (rect visible)")
    time.sleep(step_delay)

    _wait_for_state(conn, a, AnimationState.DONE, timeout=4.0)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_enable_on_trigger_edge_rising(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """EnableOnTriggerEdge enables a disabled stimulus on a rising edge, then DONE."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect enabled by rising edge on (0,20)")
    s = _make_rect(conn, x=-100, y=-100, enabled=False)

    a = conn.animations.create_enable_on_trigger_edge(
        (0, 20),
        s,
        edge=VtlEdge.RISING,
        enabled=True,
    )
    conn.animations.arm(a)

    time.sleep(0.1)
    assert conn.animations.query(a).state == AnimationState.RUNNING, (
        "should be RUNNING waiting for edge"
    )
    assert conn.stimuli.query(s).enabled is False, (
        "stimulus must still be disabled before edge"
    )

    _update_label(conn, lbl, tid, "RUNNING — waiting for rising edge")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 20), True)
    time.sleep(0.1)
    conn.vtl.set_input_line((0, 20), False)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=2.0)
    assert final == AnimationState.DONE

    assert conn.stimuli.query(s).enabled is True, (
        "stimulus should be enabled after rising edge"
    )

    _update_label(conn, lbl, tid, "rect ON (trigger fired)")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_enable_on_trigger_edge_falling(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """EnableOnTriggerEdge with FALLING edge fires on the high→low transition."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect DISABLED by falling edge on (0,21)")
    s = _make_rect(conn, x=100, y=-100, enabled=True)

    a = conn.animations.create_enable_on_trigger_edge(
        (0, 21),
        s,
        edge=VtlEdge.FALLING,
        enabled=False,
    )
    conn.animations.arm(a)

    conn.vtl.set_input_line((0, 21), True)
    time.sleep(0.1)

    _update_label(conn, lbl, tid, "RUNNING — waiting for falling edge")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 21), False)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=2.0)
    assert final == AnimationState.DONE

    assert conn.stimuli.query(s).enabled is False, (
        "stimulus should be disabled after falling edge"
    )

    _update_label(conn, lbl, tid, "rect OFF (falling edge fired)")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_couple_visibility_to_vtl_line(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """CoupleVisibility mirrors anim_enabled to the level of a VTL input line."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect coupled to VTL (0,30)")
    s = _make_rect(conn, x=0, y=-200, enabled=False)

    a = conn.animations.create_couple_visibility_to_trigger_line(
        (0, 30),
        s,
        polarity=True,
    )
    conn.animations.arm(a)

    _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    time.sleep(0.05)
    assert conn.stimuli.query(s).anim_enabled is False, (
        "anim_enabled should be False when line is LOW"
    )

    _update_label(conn, lbl, tid, "line LOW → rect OFF")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 30), True)
    time.sleep(0.1)
    assert conn.stimuli.query(s).anim_enabled is True, (
        "anim_enabled should be True when line is HIGH"
    )

    _update_label(conn, lbl, tid, "line HIGH → rect ON")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 30), False)
    time.sleep(0.1)
    assert conn.stimuli.query(s).anim_enabled is False, (
        "anim_enabled should be False when line returns LOW"
    )

    _update_label(conn, lbl, tid, "line LOW → rect OFF again")
    time.sleep(step_delay)

    conn.animations.disarm(a)
    assert conn.animations.query(a).state == AnimationState.IDLE
    assert conn.stimuli.query(s).anim_enabled is True, (
        "anim_enabled should be True after disarming"
    )

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_couple_visibility_inverted_polarity(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """CoupleVisibility with polarity=False: HIGH → anim_enabled=False, LOW → anim_enabled=True."""
    tid = request.node.name
    lbl = _label(conn, tid, "couple inverted polarity on (0,31)")
    s = _make_rect(conn, x=-250, y=-200, enabled=False)

    a = conn.animations.create_couple_visibility_to_trigger_line(
        (0, 31),
        s,
        polarity=False,
    )
    conn.animations.arm(a)

    _wait_for_state(conn, a, AnimationState.RUNNING, timeout=2.0)
    time.sleep(0.05)
    assert conn.stimuli.query(s).anim_enabled is True, (
        "inverted polarity: line LOW → anim_enabled=True"
    )

    _update_label(conn, lbl, tid, "line LOW → rect ON (inverted)")
    time.sleep(step_delay)

    conn.vtl.set_input_line((0, 31), True)
    time.sleep(0.1)
    assert conn.stimuli.query(s).anim_enabled is False, (
        "inverted polarity: line HIGH → anim_enabled=False"
    )

    _update_label(conn, lbl, tid, "line HIGH → rect OFF (inverted)")
    time.sleep(step_delay)

    conn.animations.disarm(a)
    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_move_along_path_2d(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """MoveAlongPath2D moves a stimulus through a sequence of positions."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect swept left-to-right via path")
    s = conn.stimuli.shapes.create_rect(
        pos=Vec2(-200, 0), width=60, height=60, color=Color(0.2, 0.8, 0.2)
    )

    xs = [x * 10.0 - 200.0 for x in range(41)]  # -200 → 200 in 41 steps
    ys = [0.0] * 41
    a = conn.animations.create_move_along_path_2d(
        s, x=xs, y=ys, final_action_mask=FinalAction.DISABLE
    )
    conn.animations.arm(a)

    _update_label(conn, lbl, tid, "moving left→right")
    # Wait for a few frames then confirm position has moved from the start.
    time.sleep(0.1)
    mid_info = conn.stimuli.query(s)
    assert mid_info.pos.x > -200.0, (
        f"position should have advanced from start, got x={mid_info.pos.x}"
    )

    time.sleep(step_delay * 2)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=5.0)
    assert final == AnimationState.DONE, (
        f"path animation did not complete (got {final!r})"
    )

    # After completion the final position should be the last waypoint.
    end_info = conn.stimuli.query(s)
    assert abs(end_info.pos.x - 200.0) < 1.0, (
        f"expected final x≈200, got {end_info.pos.x}"
    )
    assert abs(end_info.pos.y) < 1.0, f"expected final y≈0, got {end_info.pos.y}"

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_move_along_segments_2d(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """MoveAlongSegments2D moves at constant pixel-per-second speed along waypoints."""
    tid = request.node.name
    lbl = _label(conn, tid, "rect moving along triangle at 400 px/s")
    s = conn.stimuli.shapes.create_rect(
        pos=Vec2(-200, -100), width=50, height=50, color=Color(0.2, 0.4, 1.0)
    )

    xs = [-200.0, 200.0, 0.0, -200.0]
    ys = [-100.0, -100.0, 100.0, -100.0]
    a = conn.animations.create_move_along_segments_2d(
        s,
        x=xs,
        y=ys,
        speed_px_per_sec=400.0,
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(a)

    _update_label(conn, lbl, tid, "moving along triangle")
    # Wait a short time and confirm the stimulus has left the starting position.
    time.sleep(0.15)
    mid_info = conn.stimuli.query(s)
    assert mid_info.pos.x > -200.0, (
        f"position should have moved from start, got x={mid_info.pos.x}"
    )

    time.sleep(step_delay * 3)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=10.0)
    assert final == AnimationState.DONE, (
        f"segment animation did not complete (got {final!r})"
    )

    # After completion the final position should be the last waypoint.
    end_info = conn.stimuli.query(s)
    assert abs(end_info.pos.x - (-200.0)) < 2.0, (
        f"expected final x≈-200, got {end_info.pos.x}"
    )
    assert abs(end_info.pos.y - (-100.0)) < 2.0, (
        f"expected final y≈-100, got {end_info.pos.y}"
    )

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_final_action_restore_state(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """RESTORE_STATE final action returns stimulus to its pre-animation enabled state."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash — restore_state restores disabled rect")
    s = _make_rect(conn, x=0, y=150, enabled=False)

    a = conn.animations.create_flash(
        s, duration_frames=20, final_action_mask=FinalAction.RESTORE_STATE
    )
    conn.animations.arm(a)

    time.sleep(0.05)
    assert conn.stimuli.query(s).enabled is True, (
        "stimulus should be enabled while flash is running"
    )

    _update_label(conn, lbl, tid, "rect ON (flash running)")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=3.0)
    assert final == AnimationState.DONE

    assert conn.stimuli.query(s).enabled is False, (
        "RESTORE_STATE should restore pre-animation disabled state"
    )

    _update_label(conn, lbl, tid, "rect OFF (restored)")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_final_action_trigger_line(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """FINAL_ACTION_TRIGGER_LINE fires an output bit on a named VTL line when the animation completes."""
    tid = request.node.name
    lbl = _label(conn, tid, "flash fires VTL output on completion")

    conn.vtl.set_line_name(
        bank=0, bit=40, direction=VtlDirection.OUTPUT, name="anim_done_out"
    )

    s = _make_rect(conn, x=0, y=-150, enabled=False)
    a = conn.animations.create_flash(
        s,
        duration_frames=15,
        final_action_mask=FinalAction.FINAL_ACTION_TRIGGER_LINE | FinalAction.DISABLE,
        final_action_trigger_line="anim_done_out",
    )
    conn.animations.arm(a)

    _update_label(conn, lbl, tid, "flash running — output fires at end")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=3.0)
    assert final == AnimationState.DONE

    lines = conn.vtl.list_lines()
    assert any(l.name == "anim_done_out" for l in lines), (
        "output line should be registered"
    )

    _update_label(conn, lbl, tid, "done — output pulsed")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.vtl.set_line_name(bank=0, bit=40, direction=VtlDirection.OUTPUT, name="")
    conn.stimuli.delete(lbl)


def test_anim_list_and_query(conn: Connection) -> None:
    """list() and query() return accurate metadata for all active animations."""
    s1 = _make_rect(conn, x=-100, y=200, enabled=False)
    s2 = _make_rect(conn, x=100, y=200, enabled=False)

    a1 = conn.animations.create_flash(s1, duration_frames=120, name="flash_list_test")
    a2 = conn.animations.create_flicker(
        s2, on_frames=5, off_frames=5, name="flicker_list_test"
    )

    anim_list = conn.animations.list_animations()
    handles = {a.handle for a in anim_list}
    assert a1 in handles
    assert a2 in handles

    by_handle = {a.handle: a for a in anim_list}
    assert by_handle[a1].name == "flash_list_test"
    assert by_handle[a2].name == "flicker_list_test"
    assert by_handle[a1].state == AnimationState.IDLE
    assert by_handle[a2].state == AnimationState.IDLE

    details = conn.animations.query(a1)
    assert details.handle == a1
    assert details.name == "flash_list_test"
    assert details.state == AnimationState.IDLE
    assert s1 in details.stimuli

    conn.animations.delete(a1)
    conn.animations.delete(a2)
    conn.stimuli.delete(s1)
    conn.stimuli.delete(s2)


def test_anim_flash_with_grating(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flash works with grating stimuli (not just rects)."""
    tid = request.node.name
    lbl = _label(conn, tid, "grating enabled by flash")
    g = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=200, height=200, sf=0.04, contrast=0.9
    )
    conn.stimuli.set_enabled(g, False)

    a = conn.animations.create_flash(
        g, duration_frames=40, final_action_mask=FinalAction.DISABLE
    )
    conn.animations.arm(a)

    time.sleep(0.05)
    assert conn.stimuli.query(g).enabled is True

    _update_label(conn, lbl, tid, "grating ON (flash)")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=3.0)
    assert final == AnimationState.DONE
    assert conn.stimuli.query(g).enabled is False

    conn.animations.delete(a)
    conn.stimuli.delete(g)
    conn.stimuli.delete(lbl)


def test_anim_multiple_stimuli(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """Flash can control multiple stimuli at once."""
    tid = request.node.name
    lbl = _label(conn, tid, "three rects flashed together")
    stimuli = [_make_rect(conn, x=x, y=-50, enabled=False) for x in (-200, 0, 200)]

    a = conn.animations.create_flash(
        stimuli, duration_frames=30, final_action_mask=FinalAction.DISABLE
    )
    conn.animations.arm(a)

    time.sleep(0.05)
    for s in stimuli:
        assert conn.stimuli.query(s).enabled is True, "all three stimuli should be ON"

    _update_label(conn, lbl, tid, "three rects ON simultaneously")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=3.0)
    assert final == AnimationState.DONE
    for s in stimuli:
        assert conn.stimuli.query(s).enabled is False, (
            "all three stimuli should be OFF after flash"
        )

    conn.animations.delete(a)
    for s in stimuli:
        conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_start_action_enable(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """StartAction.ENABLE enables stimuli when animation starts; FinalAction.DISABLE disables on completion."""
    tid = request.node.name
    lbl = _label(conn, tid, "start_action=ENABLE, final=DISABLE")
    s = _make_rect(conn, x=0, y=150, enabled=False)

    # Stimulus starts disabled — start_action enables it; DISABLE final_action turns it off at the end.
    a = conn.animations.create_flash(
        s,
        duration_frames=30,
        start_action_mask=StartAction.ENABLE,
        final_action_mask=FinalAction.DISABLE,
    )
    # Normally flash enables stimuli implicitly; here we verify start_action does too.
    conn.stimuli.set_enabled(s, False)  # ensure it is still disabled before arm
    conn.animations.arm(a)

    time.sleep(0.05)
    assert conn.stimuli.query(s).enabled is True, (
        "StartAction.ENABLE should enable stimulus at start"
    )

    _update_label(conn, lbl, tid, "rect ON (start_action enabled it)")
    time.sleep(step_delay)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=3.0)
    assert final == AnimationState.DONE

    assert conn.stimuli.query(s).enabled is False, (
        "FinalAction.DISABLE should disable stimulus at end"
    )

    conn.animations.delete(a)
    conn.stimuli.delete(s)
    conn.stimuli.delete(lbl)


def test_anim_moving_bar_rf_mapping(
    conn: Connection, request: pytest.FixtureRequest, step_delay: float
) -> None:
    """RF-mapping pattern: a bar sweeps across the screen, enabled at start and disabled at end.

    This is the canonical receptive field mapping stimulus: a narrow vertical bar
    hidden until the animation fires, sweeps left-to-right at constant speed, and
    disappears automatically on completion.
    """
    tid = request.node.name
    lbl = _label(conn, tid, "RF-mapping bar: hidden→sweep L→R→hidden")

    # Narrow vertical bar, initially disabled (will be enabled by start_action).
    bar = conn.stimuli.shapes.create_rect(
        pos=Vec2(-400, 0),
        width=20,
        height=400,
        color=Color(1.0, 1.0, 1.0),
    )
    conn.stimuli.set_enabled(bar, False)
    assert not conn.stimuli.query(bar).enabled, "bar should be disabled after creation"

    # Sweep from x=-400 to x=400 at 400 px/s ≈ 2 seconds.
    a = conn.animations.create_move_along_segments_2d(
        bar,
        x=[-400.0, 400.0],
        y=[0.0, 0.0],
        speed_px_per_sec=400.0,
        name="rf_bar",
        start_action_mask=StartAction.ENABLE,
        final_action_mask=FinalAction.DISABLE,
    )
    conn.animations.arm(a)

    # After a short delay the bar should be enabled and have moved from the start.
    time.sleep(0.1)
    info = conn.stimuli.query(bar)
    assert info.enabled is True, (
        "bar should be enabled by start_action at animation start"
    )
    assert info.pos.x > -400.0, f"bar should have started moving, got x={info.pos.x}"

    _update_label(conn, lbl, tid, "bar sweeping left→right")
    time.sleep(step_delay * 2)

    final = _wait_for_state(conn, a, AnimationState.DONE, timeout=6.0)
    assert final == AnimationState.DONE, (
        f"bar animation did not complete (got {final!r})"
    )

    # At completion: final position near end waypoint, and stimulus disabled.
    end = conn.stimuli.query(bar)
    assert abs(end.pos.x - 400.0) < 5.0, (
        f"expected bar at x≈400 after sweep, got {end.pos.x}"
    )
    assert end.enabled is False, (
        "bar should be disabled by FinalAction.DISABLE after sweep"
    )

    _update_label(conn, lbl, tid, "bar done — hidden again")
    time.sleep(step_delay)

    conn.animations.delete(a)
    conn.stimuli.delete(bar)
    conn.stimuli.delete(lbl)


def test_anim_external_position_2d(conn: Connection) -> None:
    """create_external_position_2d registers the animation and returns a valid handle."""
    s = conn.stimuli.shapes.create_rect()
    a = conn.animations.create_external_position_2d(s, shm_name="/vstimd_test_ext_pos")
    assert a > 0

    details = conn.animations.query(a)
    assert details.handle == a
    assert s in details.stimuli

    conn.animations.delete(a)
    conn.stimuli.delete(s)
