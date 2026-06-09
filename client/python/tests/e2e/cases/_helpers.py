"""Shared helpers for e2e test cases."""

from __future__ import annotations

import time

from vstimd import Connection
from vstimd.animations import AnimationHandle, AnimationState
from vstimd.stimuli import StimulusHandle
from vstimd.stimuli.stimuli_models import Color, Vec2


def label(conn: Connection, test_id: str, description: str = "") -> StimulusHandle:
    """Yellow label near top of screen: '[test_id] description'."""
    text = f"[{test_id}] {description}".rstrip()
    return conn.stimuli.text.create_text(
        text=text,
        pos=Vec2(0, 260),
        box_width=900,
        box_height=50,
        letter_height=28,
        color=Color(1.0, 1.0, 0.0),
        anchor="center",
        name="_label",
    )


def update_label(
    conn: Connection, handle: StimulusHandle, test_id: str, description: str
) -> None:
    conn.stimuli.text.set_text(handle, f"[{test_id}] {description}")


def wait_for_anim_state(
    conn: Connection,
    handle: AnimationHandle,
    target: AnimationState,
    timeout: float = 3.0,
    poll_interval: float = 0.05,
) -> AnimationState:
    """Poll until the animation reaches ``target`` or ``timeout`` seconds pass."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        state = conn.animations.query(handle).state
        if state == target:
            return state
        time.sleep(poll_interval)
    return conn.animations.query(handle).state


def make_rect(
    conn: Connection, *, x: float = 0, y: float = 0, enabled: bool = True
) -> int:
    h = conn.stimuli.shapes.create_rect(
        pos=Vec2(x, y), width=80, height=80, color=Color(0.8, 0.2, 0.2)
    )
    if not enabled:
        conn.stimuli.set_enabled(h, False)
    return h
