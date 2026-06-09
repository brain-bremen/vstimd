"""E2E tests for outline/draw-mode stimulus properties."""

from __future__ import annotations

import time

import pytest

from vstimd import Connection
from vstimd.stimuli import ShapeDrawMode
from vstimd.stimuli.stimuli_models import Color, Vec2

from ._helpers import label as _label
from ._helpers import update_label as _update_label


def test_set_draw_mode_outlined(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=100, height=100)
    conn.stimuli.shapes.set_draw_mode(handle, ShapeDrawMode.OUTLINED)
    info = conn.stimuli.query(handle)
    assert info.draw_mode == ShapeDrawMode.OUTLINED
    conn.stimuli.delete(handle)


def test_set_draw_mode_filled_and_outlined(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_circle(radius=50)
    conn.stimuli.shapes.set_draw_mode(handle, ShapeDrawMode.FILLED_AND_OUTLINED)
    info = conn.stimuli.query(handle)
    assert info.draw_mode == ShapeDrawMode.FILLED_AND_OUTLINED
    conn.stimuli.delete(handle)


def test_set_outline_color_roundtrip(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=100, height=80)
    conn.stimuli.shapes.set_outline_color(handle, Color(1.0, 0.5, 0.0, 0.8))
    info = conn.stimuli.query(handle)
    assert info.outline_color.r == pytest.approx(1.0, abs=0.01)
    assert info.outline_color.g == pytest.approx(0.5, abs=0.01)
    assert info.outline_color.b == pytest.approx(0.0, abs=0.01)
    assert info.outline_color.a == pytest.approx(0.8, abs=0.01)
    conn.stimuli.delete(handle)


def test_set_outline_width_roundtrip(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_ellipse(width=120, height=80)
    conn.stimuli.shapes.set_outline_width(handle, 6.0)
    info = conn.stimuli.query(handle)
    assert info.outline_width == pytest.approx(6.0, abs=0.1)
    conn.stimuli.delete(handle)


def test_draw_mode_default_is_filled(conn: Connection) -> None:
    for h in [
        conn.stimuli.shapes.create_rect(width=100, height=100),
        conn.stimuli.shapes.create_circle(radius=50),
        conn.stimuli.shapes.create_ellipse(width=100, height=60),
    ]:
        info = conn.stimuli.query(h)
        assert info.draw_mode == ShapeDrawMode.FILLED
        conn.stimuli.delete(h)


def test_draw_mode_cycle(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=100, height=100)
    for mode in (
        ShapeDrawMode.OUTLINED,
        ShapeDrawMode.FILLED_AND_OUTLINED,
        ShapeDrawMode.FILLED,
    ):
        conn.stimuli.shapes.set_draw_mode(handle, mode)
        info = conn.stimuli.query(handle)
        assert info.draw_mode == mode
    conn.stimuli.delete(handle)


def test_outline_visual(
    conn: Connection, step_delay: float, request: pytest.FixtureRequest
) -> None:
    """Display each draw mode so a human can visually verify outlines."""
    tid = request.node.name
    conn.system.set_background(r=0.15, g=0.15, b=0.15)

    ROWS = [
        (ShapeDrawMode.FILLED, "fill only"),
        (ShapeDrawMode.OUTLINED, "outline only"),
        (ShapeDrawMode.FILLED_AND_OUTLINED, "fill + outline"),
    ]

    lbl = _label(conn, tid)
    for mode, description in ROWS:
        _update_label(conn, lbl, tid, description)
        rect = conn.stimuli.shapes.create_rect(
            pos=Vec2(-200, 0), width=180, height=120, color=Color(0.2, 0.5, 0.9)
        )
        circ = conn.stimuli.shapes.create_circle(
            pos=Vec2(0, 0), radius=70, color=Color(0.9, 0.4, 0.2)
        )
        ell = conn.stimuli.shapes.create_ellipse(
            pos=Vec2(200, 0), width=200, height=100, color=Color(0.3, 0.8, 0.3)
        )
        for h in (rect, circ, ell):
            conn.stimuli.shapes.set_draw_mode(h, mode)
            conn.stimuli.shapes.set_outline_color(h, Color(1.0, 1.0, 0.0))
            conn.stimuli.shapes.set_outline_width(h, 6.0)

        time.sleep(step_delay)

        for h in (rect, circ, ell):
            conn.stimuli.delete(h)

    conn.stimuli.delete(lbl)
    conn.system.set_background(r=0.0, g=0.0, b=0.0)


def test_outline_independent_of_fill_color(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(
        width=100, height=100, color=Color(1.0, 0.0, 0.0)
    )
    conn.stimuli.shapes.set_outline_color(handle, Color(0.0, 0.0, 1.0))
    info = conn.stimuli.query(handle)
    assert info.fill_color.r == pytest.approx(1.0, abs=0.01)
    assert info.outline_color.b == pytest.approx(1.0, abs=0.01)

    conn.stimuli.set_fill_color(handle, Color(0.0, 1.0, 0.0))
    info = conn.stimuli.query(handle)
    assert info.fill_color.g == pytest.approx(1.0, abs=0.01)
    assert info.outline_color.b == pytest.approx(1.0, abs=0.01)
    conn.stimuli.delete(handle)
