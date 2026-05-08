"""Shared e2e test cases. Imported by test_e2e.py and test_e2e_null.py.

Each function receives a `conn` fixture from the importing test module,
so the same cases run against both a real and a null-renderer server.
"""

import time

import pytest

from wonderlamp import Connection
from wonderlamp.stimuli import RectParams, StimulusType


def test_create_rect(conn: Connection) -> None:
    handle = conn.stimuli.create_rect(x=0, y=0, width=100, height=100, r=1.0, g=0.0, b=0.0)
    assert handle > 0

    info = conn.stimuli.query(handle)
    assert info.stimulus_type == StimulusType.RECT
    assert isinstance(info.params, RectParams)
    assert info.params.width == pytest.approx(100.0, abs=0.5)
    assert info.params.height == pytest.approx(100.0, abs=0.5)
    assert info.fill_color.r == pytest.approx(1.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.b == pytest.approx(0.0, abs=0.01)

    time.sleep(1.0)
    conn.stimuli.delete(handle)
