"""Shared visual API test cases (psychopy-compatible).

Imported by test_visual.py (real server) and test_visual_null.py (null server).
Each function receives a `win` fixture — a vstimd.psychopy.visual.Window — so the
same cases run against both backends.
"""

import time

import pytest

import vstimd.psychopy.visual as visual
from vstimd.stimuli import DiscParams, GratingMask, GratingParams, GratingTexture, RectParams, StimulusType


def test_create_rect(win: visual.Window, step_delay: float) -> None:
    """Create a simple red rectangle and verify its properties."""
    rect = visual.Rect(win, width=200, height=100, fillColor="red", autoDraw=True)

    info = win._conn.stimuli.query(rect._handle)
    assert info.stimulus_type == StimulusType.RECT
    assert isinstance(info.params, RectParams)
    assert info.params.width == pytest.approx(200.0, abs=0.5)
    assert info.params.height == pytest.approx(100.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(1.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(0.0, abs=0.01)

    win.flip()
    time.sleep(step_delay)
    rect.autoDraw = False


def test_rect_position_size(win: visual.Window, step_delay: float) -> None:
    """Test rectangle at different positions and sizes."""
    # Large rectangle at center
    rect = visual.Rect(
        win, width=400, height=300, fillColor="blue", pos=(0, 0), autoDraw=True
    )
    win.flip()
    time.sleep(step_delay)

    # Small rectangle top-right
    rect.size = (100, 100)
    rect.pos = (300, 200)
    rect.fillColor = "green"
    win.flip()
    time.sleep(step_delay)

    # Small rectangle bottom-left
    rect.pos = (-300, -200)
    rect.fillColor = "yellow"
    win.flip()
    time.sleep(step_delay)

    rect.autoDraw = False


def test_rect_colors(win: visual.Window, step_delay: float) -> None:
    """Test rectangle with different colors."""
    rect = visual.Rect(win, width=200, height=200, fillColor="red", autoDraw=True)

    # Red
    win.flip()
    time.sleep(step_delay)

    # Green
    rect.fillColor = "green"
    win.flip()
    time.sleep(step_delay)

    # Blue
    rect.fillColor = "blue"
    win.flip()
    time.sleep(step_delay)

    # White
    rect.fillColor = "white"
    win.flip()
    time.sleep(step_delay)

    # RGB tuple
    rect.fillColor = (1.0, 0.5, 0.0)  # Orange
    win.flip()
    time.sleep(step_delay)

    rect.autoDraw = False


def test_rect_opacity(win: visual.Window, step_delay: float) -> None:
    """Test rectangle opacity/transparency."""
    rect1 = visual.Rect(
        win, width=300, height=300, fillColor="red", pos=(-100, 0), autoDraw=True
    )
    rect2 = visual.Rect(
        win, width=300, height=300, fillColor="blue", pos=(100, 0), autoDraw=True
    )

    # Both opaque
    win.flip()
    time.sleep(step_delay)

    # Blue semi-transparent
    rect2.opacity = 0.5
    win.flip()
    time.sleep(step_delay)

    # Both semi-transparent
    rect1.opacity = 0.7
    win.flip()
    time.sleep(step_delay)

    rect1.autoDraw = False
    rect2.autoDraw = False


def test_create_circle(win: visual.Window, step_delay: float) -> None:
    """Create a simple blue circle and verify its properties."""
    circle = visual.Circle(win, radius=50, fillColor="blue", autoDraw=True)

    info = win._conn.stimuli.query(circle._handle)
    assert info.stimulus_type == StimulusType.DISC
    assert isinstance(info.params, DiscParams)
    assert info.params.radius == pytest.approx(50.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(1.0, abs=0.01)

    win.flip()
    time.sleep(step_delay)
    circle.autoDraw = False


def test_circle_sizes(win: visual.Window, step_delay: float) -> None:
    """Test circles at different sizes and positions."""
    # Large circle at center
    circle = visual.Circle(win, radius=150, fillColor="red", pos=(0, 0), autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    # Small circle top-left
    circle.radius = 50
    circle.pos = (-200, 150)
    circle.fillColor = "green"
    win.flip()
    time.sleep(step_delay)

    # Medium circle bottom-right
    circle.radius = 100
    circle.pos = (200, -150)
    circle.fillColor = "yellow"
    win.flip()
    time.sleep(step_delay)

    circle.autoDraw = False

    # Multiple circles at once
    c1 = visual.Circle(win, radius=60, fillColor="red", pos=(-150, 0), autoDraw=True)
    c2 = visual.Circle(win, radius=60, fillColor="green", pos=(0, 0), autoDraw=True)
    c3 = visual.Circle(win, radius=60, fillColor="blue", pos=(150, 0), autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    c1.autoDraw = False
    c2.autoDraw = False
    c3.autoDraw = False


def test_create_grating_default(win: visual.Window, step_delay: float) -> None:
    grat = visual.GratingStim(win, tex="sin", size=200, autoDraw=True)

    info = win._conn.stimuli.query(grat._handle)
    assert info.stimulus_type == StimulusType.GRATING
    assert isinstance(info.params, GratingParams)
    assert info.params.waveform == GratingTexture.SIN
    assert info.params.mask == GratingMask.NONE
    assert info.params.contrast == pytest.approx(1.0, abs=0.01)
    assert info.params.drift_coupled is True

    win.flip()
    time.sleep(step_delay)
    grat.autoDraw = False


def test_create_grating_sqr_circle_mask(win: visual.Window, step_delay: float) -> None:
    grat = visual.GratingStim(
        win,
        tex="sqr",
        mask="circle",
        size=(300, 300),
        sf=0.03,
        phase=0.1,
        ori=30.0,
        color="white",
        contrast=0.75,
        autoDraw=True,
    )

    info = win._conn.stimuli.query(grat._handle)
    assert info.stimulus_type == StimulusType.GRATING
    assert isinstance(info.params, GratingParams)
    assert info.params.waveform == GratingTexture.SQR
    assert info.params.mask == GratingMask.CIRCLE
    assert info.params.sf == pytest.approx(0.03, rel=1e-2)
    assert info.params.phase == pytest.approx(0.1, abs=0.01)
    assert info.params.contrast == pytest.approx(0.75, abs=0.01)

    win.flip()
    time.sleep(step_delay)
    grat.autoDraw = False


def test_grating_mutate_sf_phase_contrast(
    win: visual.Window, step_delay: float
) -> None:
    grat = visual.GratingStim(win, tex="sin", size=200, sf=0.05, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    grat.sf = 0.1
    grat.phase = 0.5
    grat.contrast = 0.6
    win.flip()
    time.sleep(step_delay)

    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.sf == pytest.approx(0.1, rel=1e-2)
    assert info.params.phase == pytest.approx(0.5, abs=0.01)
    assert info.params.contrast == pytest.approx(0.6, abs=0.01)

    grat.autoDraw = False


def test_grating_drift_extension(win: visual.Window, step_delay: float) -> None:
    grat = visual.GratingStim(win, tex="sin", size=200, drift_speed=1.5, autoDraw=True)
    win.flip()
    time.sleep(step_delay * 3)

    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_speed == pytest.approx(1.5, abs=0.01)
    assert info.params.drift_coupled is True

    grat.drift_decoupled = True
    grat.drift_angle = 45.0
    win.flip()
    time.sleep(step_delay * 3)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_coupled is False
    assert info.params.drift_angle == pytest.approx(45.0, abs=0.1)

    grat.autoDraw = False


def test_grating_autodraw(win: visual.Window, step_delay: float) -> None:
    grat = visual.GratingStim(win, tex="sin", size=100, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    info = win._conn.stimuli.query(grat._handle)
    assert info.enabled is True

    grat.autoDraw = False
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert info.enabled is False


def test_grating_two_color_create(win: visual.Window, step_delay: float) -> None:
    """GratingStim created with foreColor=red, backColor=blue is round-tripped correctly."""
    grat = visual.GratingStim(
        win, tex="sin", size=200,
        color=(1.0, 0.0, 0.0),       # foreColor = red (in rgb1 space)
        colorSpace="rgb1",
        backColor=(0.0, 0.0, 1.0),   # backColor = blue (in rgb1 space)
        autoDraw=True,
    )
    win.flip()
    time.sleep(step_delay)

    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.fore_color[2] == pytest.approx(0.0, abs=0.01)
    assert info.params.fore_color[3] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[0] == pytest.approx(0.0, abs=0.01)
    assert info.params.back_color[2] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(1.0, abs=0.01)

    grat.autoDraw = False


def test_grating_color_setters(win: visual.Window, step_delay: float) -> None:
    """Setting color, foreColor, backColor and opacity post-creation updates the server."""
    grat = visual.GratingStim(win, tex="sin", size=200, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    # color setter (= foreColor)
    grat.color = (0.5, 0.25, 0.0)
    grat.colorSpace = "rgb1"  # type: ignore[attr-defined]  # noqa: SIM117
    win.flip()
    time.sleep(step_delay)

    # foreColor alias
    grat.foreColor = (1.0, 0.0, 0.0)
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.fore_color[1] == pytest.approx(0.0, abs=0.01)

    # backColor setter
    grat.backColor = (0.0, 0.0, 1.0)
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.back_color[2] == pytest.approx(1.0, abs=0.01)

    # opacity setter (global)
    grat.opacity = 0.5
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert info.params.opacity == pytest.approx(0.5, abs=0.01)
    # fore/back alpha unaffected by global opacity change
    assert info.params.fore_color[3] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(1.0, abs=0.01)

    grat.autoDraw = False


def test_grating_ori(win: visual.Window, step_delay: float) -> None:
    grat = visual.GratingStim(win, tex="sin", size=200, ori=45.0, autoDraw=True)
    win.flip()
    time.sleep(step_delay)
    assert grat.ori == pytest.approx(45.0, abs=0.01)

    grat.ori = 90.0
    win.flip()
    time.sleep(step_delay)
    assert grat.ori == pytest.approx(90.0, abs=0.01)

    grat.autoDraw = False
