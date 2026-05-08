"""Shared visual API test cases (psychopy-compatible).

Imported by test_visual.py (real server) and test_visual_null.py (null server).
Each function receives a `win` fixture — a wonderlamp.psychopy.visual.Window — so the
same cases run against both backends.
"""

import pytest

import wonderlamp.psychopy.visual as visual
from wonderlamp.stimuli import DiscParams, RectParams, StimulusType


def test_create_rect(win: visual.Window) -> None:
    rect = visual.Rect(win, width=200, height=100, fillColor="red")

    info = win._conn.stimuli.query(rect._handle)
    assert info.stimulus_type == StimulusType.RECT
    assert isinstance(info.params, RectParams)
    assert info.params.width == pytest.approx(200.0, abs=0.5)
    assert info.params.height == pytest.approx(100.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(1.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(0.0, abs=0.01)

    rect.draw()
    win.flip()


def test_create_circle(win: visual.Window) -> None:
    circle = visual.Circle(win, radius=50, fillColor="blue")

    info = win._conn.stimuli.query(circle._handle)
    assert info.stimulus_type == StimulusType.DISC
    assert isinstance(info.params, DiscParams)
    assert info.params.radius == pytest.approx(50.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(1.0, abs=0.01)

    circle.draw()
    win.flip()
