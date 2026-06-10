"""E2E tests for ellipse stimuli (ellipse.proto: CreateEllipseRequest, SetEllipseSizeRequest)."""
from __future__ import annotations

import pytest

from vstimd import Connection
from vstimd.stimuli import EllipseParams, StimulusType
from vstimd.stimuli.stimuli_models import Color, Vec2


def test_create_ellipse(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_ellipse(
        pos=Vec2(0, 0), width=200, height=80, color=Color(0.0, 1.0, 0.0)
    )
    assert handle > 0

    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.ELLIPSE
    assert isinstance(info.params, EllipseParams)
    assert info.params.width == pytest.approx(200.0, abs=0.5)
    assert info.params.height == pytest.approx(80.0, abs=0.5)
    assert info.fill_color.g == pytest.approx(1.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_create_ellipse_with_angle(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_ellipse(width=150, height=50, angle=45.0)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, EllipseParams)
    assert info.orientation == pytest.approx(45.0, abs=0.1)
    conn.stimuli.delete(handle)


def test_set_ellipse_size(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_ellipse(width=100, height=50)
    conn.stimuli.shapes.set_ellipse_size(handle, 300, 120)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, EllipseParams)
    assert info.params.width == pytest.approx(300.0, abs=0.5)
    assert info.params.height == pytest.approx(120.0, abs=0.5)
    conn.stimuli.delete(handle)
