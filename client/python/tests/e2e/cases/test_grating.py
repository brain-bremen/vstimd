"""E2E tests for grating stimuli."""
from __future__ import annotations

import time

import pytest

from vstimd import Connection
from vstimd.stimuli import GratingMask, GratingParams, GratingTexture, StimulusType
from vstimd.stimuli.stimuli_models import Color, Vec2
from ._helpers import label as _label, update_label as _update_label


def test_create_grating(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=200, height=200, sf=0.05, phase=0.25, angle=45.0,
        contrast=0.8, fore_color=Color(0.0, 1.0, 0.0),
        waveform=GratingTexture.SQR, mask=GratingMask.CIRCLE,
    )
    assert handle > 0

    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.GRATING
    assert isinstance(info.params, GratingParams)
    assert info.params.width == pytest.approx(200.0, abs=0.5)
    assert info.params.height == pytest.approx(200.0, abs=0.5)
    assert info.params.sf == pytest.approx(0.05, rel=1e-3)
    assert info.params.phase == pytest.approx(0.25, abs=0.01)
    assert info.params.contrast == pytest.approx(0.8, abs=0.01)
    assert info.params.waveform == GratingTexture.SQR
    assert info.params.mask == GratingMask.CIRCLE

    conn.stimuli.delete(handle)


def test_grating_mutate_phase(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(sf=0.05)
    conn.stimuli.grating.set_phase(handle, 0.5)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.phase == pytest.approx(0.5, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_mutate_sf(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(sf=0.05)
    conn.stimuli.grating.set_sf(handle, 0.1)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.sf == pytest.approx(0.1, rel=1e-3)
    conn.stimuli.delete(handle)


def test_grating_mutate_contrast(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(sf=0.05)
    conn.stimuli.grating.set_contrast(handle, 0.5)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.contrast == pytest.approx(0.5, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_mutate_waveform(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(waveform=GratingTexture.SIN)
    conn.stimuli.grating.set_waveform(handle, GratingTexture.SAW)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.waveform == GratingTexture.SAW
    conn.stimuli.delete(handle)


def test_grating_set_mask(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(mask=GratingMask.NONE)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.mask == GratingMask.NONE

    conn.stimuli.grating.set_mask(handle, GratingMask.CIRCLE)
    info = conn.stimuli.query(handle)
    assert info.params.mask == GratingMask.CIRCLE

    conn.stimuli.delete(handle)


def test_grating_drift_speed(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(sf=0.05, drift_speed=2.0)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_speed == pytest.approx(2.0, abs=0.01)
    assert info.params.drift_coupled is True

    conn.stimuli.grating.set_drift_speed(handle, 0.0)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_speed == pytest.approx(0.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_drift_decoupled(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(sf=0.05, drift_decoupled=True, drift_angle=90.0)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_coupled is False
    assert info.params.drift_angle == pytest.approx(90.0, abs=0.1)

    conn.stimuli.grating.set_drift_decoupled(handle, False)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_coupled is True
    conn.stimuli.delete(handle)


def test_grating_visual(conn: Connection, step_delay: float, request: pytest.FixtureRequest) -> None:
    """Display grating parameter variations sequentially, one row at a time."""
    tid = request.node.name
    PATCH_W, PATCH_H = 200, 150
    COL_STEP = 230

    _SF       = 0.05
    _WAVEFORM = GratingTexture.SIN
    _MASK     = GratingMask.NONE

    ROWS: list[tuple[str, list[dict]]] = [
        ("spatial frequency", [{"sf": sf} for sf in [0.01, 0.03, 0.05, 0.07, 0.10]]),
        ("contrast",          [{"contrast": c} for c in [0.2, 0.4, 0.6, 0.8, 1.0]]),
        ("phase",             [{"phase": p} for p in [0.0, 0.25, 0.5, 0.75, 1.0]]),
        ("orientation",       [{"angle": a} for a in [0.0, 45.0, 90.0, 135.0, 180.0]]),
        ("waveform",          [{"waveform": w} for w in [
            GratingTexture.SIN, GratingTexture.SQR,
            GratingTexture.SAW, GratingTexture.TRI,
        ]]),
        ("mask",              [{"mask": m} for m in [
            GratingMask.NONE, GratingMask.CIRCLE,
            GratingMask.GAUSS, GratingMask.HANN, GratingMask.RAISED_COS,
        ]]),
    ]

    conn.system.set_background(r=0.4, g=0.4, b=0.4)
    lbl = _label(conn, tid)

    for row_name, patches in ROWS:
        n = len(patches)
        xs = [(j - (n - 1) / 2) * COL_STEP for j in range(n)]
        handles: list[int] = []

        _update_label(conn, lbl, tid, row_name)

        for x, overrides in zip(xs, patches):
            base: dict = dict(
                pos=Vec2(x, 0), width=PATCH_W, height=PATCH_H,
                sf=_SF, phase=0.0, angle=0.0,
                contrast=1.0, waveform=_WAVEFORM, mask=_MASK,
            )
            base.update(overrides)
            h = conn.stimuli.grating.create_grating(**base)
            assert h > 0
            handles.append(h)

        time.sleep(step_delay)

        for h in handles:
            conn.stimuli.delete(h)

    # Assertions via fresh single-grating queries.
    h_sf = conn.stimuli.grating.create_grating(pos=Vec2(0, 0), width=PATCH_W, height=PATCH_H, sf=0.05)
    info = conn.stimuli.query(h_sf)
    assert isinstance(info.params, GratingParams)
    assert info.params.sf == pytest.approx(0.05, rel=1e-3)
    conn.stimuli.delete(h_sf)

    h_wf = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=PATCH_W, height=PATCH_H, waveform=GratingTexture.SQR
    )
    info = conn.stimuli.query(h_wf)
    assert isinstance(info.params, GratingParams)
    assert info.params.waveform == GratingTexture.SQR
    conn.stimuli.delete(h_wf)

    # Drift animation.
    drift_handle = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=300, height=300, sf=0.05, contrast=1.0,
    )
    assert drift_handle > 0

    _update_label(conn, lbl, tid, "drift (coupled)")
    conn.stimuli.grating.set_drift_speed(drift_handle, 1.0)
    info = conn.stimuli.query(drift_handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.drift_speed == pytest.approx(1.0, abs=0.01)
    assert info.params.drift_coupled is True
    time.sleep(step_delay * 3)

    _update_label(conn, lbl, tid, "drift (reverse)")
    conn.stimuli.grating.set_drift_speed(drift_handle, -1.0)
    time.sleep(step_delay * 3)

    _update_label(conn, lbl, tid, "drift (decoupled 90°)")
    conn.stimuli.grating.set_drift_decoupled(drift_handle, True)
    conn.stimuli.grating.set_drift_angle(drift_handle, 90.0)
    info = conn.stimuli.query(drift_handle)
    assert info.params.drift_coupled is False
    assert info.params.drift_angle == pytest.approx(90.0, abs=0.1)
    time.sleep(step_delay * 3)

    conn.stimuli.grating.set_drift_speed(drift_handle, 0.0)
    conn.stimuli.grating.set_drift_decoupled(drift_handle, False)
    info = conn.stimuli.query(drift_handle)
    assert info.params.drift_speed == pytest.approx(0.0, abs=0.01)
    assert info.params.drift_coupled is True

    conn.stimuli.delete(drift_handle)
    conn.stimuli.delete(lbl)
    conn.system.set_background(r=0.0, g=0.0, b=0.0)


def test_grating_two_color_create(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=200, height=200,
        fore_color=Color(1.0, 0.0, 0.0),
        back_color=Color(0.0, 0.0, 1.0),
    )
    assert handle > 0

    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.GRATING
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.fore_color[1] == pytest.approx(0.0, abs=0.01)
    assert info.params.fore_color[2] == pytest.approx(0.0, abs=0.01)
    assert info.params.fore_color[3] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[0] == pytest.approx(0.0, abs=0.01)
    assert info.params.back_color[1] == pytest.approx(0.0, abs=0.01)
    assert info.params.back_color[2] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(1.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_mutate_fore_color(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating()
    conn.stimuli.grating.set_fore_color(handle, Color(0.5, 0.25, 0.0, 0.7))
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(0.5, abs=0.01)
    assert info.params.fore_color[1] == pytest.approx(0.25, abs=0.01)
    assert info.params.fore_color[2] == pytest.approx(0.0, abs=0.01)
    assert info.params.fore_color[3] == pytest.approx(0.7, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_mutate_back_color(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating()
    conn.stimuli.grating.set_back_color(handle, Color(0.1, 0.2, 0.3, 0.4))
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.back_color[0] == pytest.approx(0.1, abs=0.01)
    assert info.params.back_color[1] == pytest.approx(0.2, abs=0.01)
    assert info.params.back_color[2] == pytest.approx(0.3, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(0.4, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_mutate_opacity(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating()
    conn.stimuli.grating.set_opacity(handle, 0.4)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.opacity == pytest.approx(0.4, abs=0.01)
    assert info.opacity == pytest.approx(0.4, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_fore_back_color_independent(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(
        fore_color=Color(1.0, 0.0, 0.0), back_color=Color(0.0, 1.0, 0.0)
    )
    conn.stimuli.grating.set_fore_color(handle, Color(0.0, 0.0, 1.0))
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[2] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[1] == pytest.approx(1.0, abs=0.01)

    conn.stimuli.grating.set_back_color(handle, Color(1.0, 1.0, 0.0))
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[2] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[1] == pytest.approx(1.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_per_color_alpha(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(
        fore_color=Color(1.0, 0.0, 0.0, 0.5),
        back_color=Color(0.0, 0.0, 1.0, 0.0),
        opacity=0.8,
    )
    assert handle > 0
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, GratingParams)
    assert info.params.fore_color[0] == pytest.approx(1.0, abs=0.01)
    assert info.params.fore_color[3] == pytest.approx(0.5, abs=0.01)
    assert info.params.back_color[2] == pytest.approx(1.0, abs=0.01)
    assert info.params.back_color[3] == pytest.approx(0.0, abs=0.01)
    assert info.opacity == pytest.approx(0.8, abs=0.01)
    conn.stimuli.delete(handle)


def test_grating_opacity(conn: Connection) -> None:
    handle = conn.stimuli.grating.create_grating(
        pos=Vec2(0, 0), width=200, height=200,
        fore_color=Color(1.0, 0.0, 0.0), opacity=0.5,
    )
    assert handle > 0
    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.GRATING
    assert isinstance(info.params, GratingParams)
    assert info.opacity == pytest.approx(0.5, abs=0.01)
    conn.stimuli.delete(handle)
