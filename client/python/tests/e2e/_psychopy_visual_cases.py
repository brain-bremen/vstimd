"""Shared visual API test cases (psychopy-compatible).

Imported by test_visual.py (real server) and test_visual_null.py (null server).
Each function receives a `win` fixture — a vstimd.psychopy.visual.Window — so the
same cases run against both backends.
"""

import time

import pytest

import vstimd.psychopy.visual as visual
from vstimd.stimuli import CircleParams, GratingMask, GratingParams, GratingTexture, RectParams, StimulusType
from vstimd.stimuli.stimuli_models import Color, Vec2


# ── Label helpers ─────────────────────────────────────────────────────────────

def _label(win: visual.Window, test_id: str, description: str = "") -> int:
    """Yellow label near top of screen: '[test_id] description'.

    Pass request.node.name as test_id so the label is always searchable.
    """
    text = f"[{test_id}] {description}".rstrip()
    return win._conn.stimuli.create_text(
        text=text, pos=Vec2(0, 260),
        box_width=900, box_height=50,
        letter_height=28,
        color=Color(1.0, 1.0, 0.0),
        anchor="center",
        name="_label",
    )


def _update_label(win: visual.Window, handle: int, test_id: str, description: str) -> None:
    win._conn.stimuli.set_text(handle, f"[{test_id}] {description}")


# ── Rect tests ────────────────────────────────────────────────────────────────

def test_create_rect(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "red 200×100 rect")
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
    win._conn.stimuli.delete(label)


def test_rect_position_size(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "blue 400×300 at centre")
    rect = visual.Rect(win, width=400, height=300, fillColor="blue", pos=(0, 0), autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "green 100×100 top-right")
    rect.size = (100, 100)
    rect.pos = (300, 200)
    rect.fillColor = "green"
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "yellow 100×100 bottom-left")
    rect.pos = (-300, -200)
    rect.fillColor = "yellow"
    win.flip()
    time.sleep(step_delay)

    rect.autoDraw = False
    win._conn.stimuli.delete(label)


def test_rect_colors(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "red")
    rect = visual.Rect(win, width=200, height=200, fillColor="red", autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    for color, name in [("green", "green"), ("blue", "blue"), ("white", "white"),
                        ((1.0, 0.5, 0.0), "orange (rgb1 tuple)")]:
        _update_label(win, label, tid, name)
        rect.fillColor = color
        win.flip()
        time.sleep(step_delay)

    rect.autoDraw = False
    win._conn.stimuli.delete(label)


def test_rect_opacity(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "red + blue, both opaque")
    rect1 = visual.Rect(win, width=300, height=300, fillColor="red", pos=(-100, 0), autoDraw=True)
    rect2 = visual.Rect(win, width=300, height=300, fillColor="blue", pos=(100, 0), autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "blue semi-transparent (0.5)")
    rect2.opacity = 0.5
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "both semi-transparent (0.7 / 0.5)")
    rect1.opacity = 0.7
    win.flip()
    time.sleep(step_delay)

    rect1.autoDraw = False
    rect2.autoDraw = False
    win._conn.stimuli.delete(label)


# ── Circle tests ──────────────────────────────────────────────────────────────

def test_create_circle(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "blue r=50")
    circle = visual.Circle(win, radius=50, fillColor="blue", autoDraw=True)

    info = win._conn.stimuli.query(circle._handle)
    assert info.stimulus_type == StimulusType.CIRCLE
    assert isinstance(info.params, CircleParams)
    assert info.params.radius == pytest.approx(50.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(1.0, abs=0.01)

    win.flip()
    time.sleep(step_delay)
    circle.autoDraw = False
    win._conn.stimuli.delete(label)


def test_circle_sizes(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "red r=150 at centre")
    circle = visual.Circle(win, radius=150, fillColor="red", pos=(0, 0), autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "green r=50 top-left")
    circle.radius = 50
    circle.pos = (-200, 150)
    circle.fillColor = "green"
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "yellow r=100 bottom-right")
    circle.radius = 100
    circle.pos = (200, -150)
    circle.fillColor = "yellow"
    win.flip()
    time.sleep(step_delay)

    circle.autoDraw = False

    _update_label(win, label, tid, "RGB trio r=60")
    c1 = visual.Circle(win, radius=60, fillColor="red",   pos=(-150, 0), autoDraw=True)
    c2 = visual.Circle(win, radius=60, fillColor="green", pos=(0, 0),    autoDraw=True)
    c3 = visual.Circle(win, radius=60, fillColor="blue",  pos=(150, 0),  autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    c1.autoDraw = False
    c2.autoDraw = False
    c3.autoDraw = False
    win._conn.stimuli.delete(label)


# ── Grating tests ─────────────────────────────────────────────────────────────

def test_create_grating_default(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "sin, size=200, no mask")
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
    win._conn.stimuli.delete(label)


def test_create_grating_sqr_circle_mask(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "sqr, circle mask, 30°, sf=0.03")
    grat = visual.GratingStim(
        win, tex="sqr", mask="circle", size=(300, 300),
        sf=0.03, phase=0.1, ori=30.0, color="white", contrast=0.75, autoDraw=True,
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
    win._conn.stimuli.delete(label)


def test_grating_mutate_sf_phase_contrast(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "sin sf=0.05")
    grat = visual.GratingStim(win, tex="sin", size=200, sf=0.05, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "sf=0.1, phase=0.5, contrast=0.6")
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
    win._conn.stimuli.delete(label)


def test_grating_drift_extension(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "coupled, speed=1.5")
    grat = visual.GratingStim(win, tex="sin", size=200, drift_speed=1.5, autoDraw=True)
    win.flip()
    time.sleep(step_delay * 3)

    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_speed == pytest.approx(1.5, abs=0.01)
    assert info.params.drift_coupled is True

    _update_label(win, label, tid, "decoupled, angle=45°")
    grat.drift_decoupled = True
    grat.drift_angle = 45.0
    win.flip()
    time.sleep(step_delay * 3)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_coupled is False
    assert info.params.drift_angle == pytest.approx(45.0, abs=0.1)

    grat.autoDraw = False
    win._conn.stimuli.delete(label)


def test_grating_autodraw(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "sin visible (autoDraw=True)")
    grat = visual.GratingStim(win, tex="sin", size=100, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    info = win._conn.stimuli.query(grat._handle)
    assert info.enabled is True

    _update_label(win, label, tid, "hidden (autoDraw=False)")
    grat.autoDraw = False
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert info.enabled is False

    win._conn.stimuli.delete(label)


def test_grating_two_color_create(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "red/blue fore/back")
    grat = visual.GratingStim(
        win, tex="sin", size=200,
        color=(1.0, 0.0, 0.0), colorSpace="rgb1",
        backColor=(0.0, 0.0, 1.0), autoDraw=True,
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
    win._conn.stimuli.delete(label)


def test_grating_color_setters(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "default sin")
    grat = visual.GratingStim(win, tex="sin", size=200, autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "foreColor orange")
    grat.color = (0.5, 0.25, 0.0)
    grat.colorSpace = "rgb1"
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "foreColor red")
    grat.foreColor = (1.0, 0.0, 0.0)
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.fore_color[1] == pytest.approx(0.0, abs=0.01)

    _update_label(win, label, tid, "backColor blue")
    grat.backColor = (0.0, 0.0, 1.0)
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.back_color[2] == pytest.approx(1.0, abs=0.01)

    _update_label(win, label, tid, "opacity=0.5")
    grat.opacity = 0.5
    win.flip()
    time.sleep(step_delay)
    info = win._conn.stimuli.query(grat._handle)
    assert info.params.opacity == pytest.approx(0.5, abs=0.01)
    assert info.params.fore_color[3] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(1.0, abs=0.01)

    grat.autoDraw = False
    win._conn.stimuli.delete(label)


def test_grating_ori(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "45°")
    grat = visual.GratingStim(win, tex="sin", size=200, ori=45.0, autoDraw=True)
    win.flip()
    time.sleep(step_delay)
    assert grat.ori == pytest.approx(45.0, abs=0.01)

    _update_label(win, label, tid, "90°")
    grat.ori = 90.0
    win.flip()
    time.sleep(step_delay)
    assert grat.ori == pytest.approx(90.0, abs=0.01)

    grat.autoDraw = False
    win._conn.stimuli.delete(label)


# ── TextBox2 tests ─────────────────────────────────────────────────────────────

def test_create_textbox2(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "white 'Hello vstimd'")
    tb = visual.TextBox2(
        win, text="Hello vstimd",
        pos=(0, 0), size=(600, 100), letterHeight=56,
        color="white", autoDraw=True,
    )
    win.flip()
    time.sleep(step_delay)
    tb.autoDraw = False
    win._conn.stimuli.delete(label)


def test_textbox2_text_update(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "'Before'")
    tb = visual.TextBox2(win, text="Before", pos=(0, 0),
                         size=(600, 100), letterHeight=56,
                         color="white", autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    _update_label(win, label, tid, "'After'")
    tb.text = "After"
    win.flip()
    time.sleep(step_delay)

    tb.autoDraw = False
    win._conn.stimuli.delete(label)


def test_textbox2_colors(win: visual.Window, step_delay: float, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    label = _label(win, tid, "white")
    tb = visual.TextBox2(win, text="Color test", pos=(0, 0),
                         size=(500, 100), letterHeight=56,
                         color="white", autoDraw=True)
    win.flip()
    time.sleep(step_delay)

    for color, name in [("red", "red"), ("cyan", "cyan"), ("yellow", "yellow")]:
        _update_label(win, label, tid, name)
        tb.color = color
        win.flip()
        time.sleep(step_delay)

    tb.autoDraw = False
    win._conn.stimuli.delete(label)
