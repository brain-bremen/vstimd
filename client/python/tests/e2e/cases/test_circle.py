"""E2E tests for circle stimuli (circle.proto: CreateCircleRequest, SetCircleRadiusRequest)."""
from __future__ import annotations

import pytest

from vstimd import Connection
from vstimd.stimuli import CircleParams, StimulusType
from vstimd.stimuli.stimuli_models import Color, Vec2


def test_create_circle(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_circle(pos=Vec2(0, 0), radius=60, color=Color(0.0, 0.0, 1.0))
    assert handle > 0

    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.CIRCLE
    assert isinstance(info.params, CircleParams)
    assert info.params.radius == pytest.approx(60.0, abs=0.5)
    assert info.fill_color.b == pytest.approx(1.0, abs=0.01)
    assert info.fill_color.r == pytest.approx(0.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_set_circle_radius(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_circle(radius=40)
    conn.stimuli.shapes.set_circle_radius(handle, 90)
    info = conn.stimuli.query(handle)
    assert isinstance(info.params, CircleParams)
    assert info.params.radius == pytest.approx(90.0, abs=0.5)
    conn.stimuli.delete(handle)
