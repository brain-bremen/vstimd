"""E2E tests for QueryStimulusRequest (query.proto)."""
from __future__ import annotations

import pytest

from vstimd import Connection
from vstimd.stimuli.stimuli_models import Color, Vec2


def test_query_pos(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(pos=Vec2(120, -80), width=50, height=50)
    info = conn.stimuli.query(handle)
    assert info.pos.x == pytest.approx(120.0, abs=0.5)
    assert info.pos.y == pytest.approx(-80.0, abs=0.5)
    conn.stimuli.delete(handle)


def test_query_enabled(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    info = conn.stimuli.query(handle)
    assert info.enabled is True

    conn.stimuli.set_enabled(handle, False)
    info = conn.stimuli.query(handle)
    assert info.enabled is False
    conn.stimuli.delete(handle)


def test_query_opacity(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.set_alpha(handle, 0.3)
    info = conn.stimuli.query(handle)
    assert info.opacity == pytest.approx(0.3, abs=0.01)
    conn.stimuli.delete(handle)


def test_query_fill_color(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(color=Color(1.0, 0.0, 0.0))
    conn.stimuli.set_fill_color(handle, Color(0.0, 0.5, 1.0))
    info = conn.stimuli.query(handle)
    assert info.fill_color.r == pytest.approx(0.0, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.5, abs=0.01)
    assert info.fill_color.b == pytest.approx(1.0, abs=0.01)
    conn.stimuli.delete(handle)


def test_query_orientation(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.set_orientation(handle, 45.0)
    info = conn.stimuli.query(handle)
    assert info.orientation == pytest.approx(45.0, abs=0.1)
    conn.stimuli.delete(handle)


def test_query_draw_order(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_rect()
    info1 = conn.stimuli.query(h1)
    info2 = conn.stimuli.query(h2)
    assert info2.draw_order > info1.draw_order
    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


def test_query_id_stable(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    id1 = conn.stimuli.query(handle).id
    id2 = conn.stimuli.query(handle).id
    assert id1 == id2
    assert len(id1) > 0
    conn.stimuli.delete(handle)


def test_query_client_uuid(conn: Connection) -> None:
    import uuid as uuid_mod
    client_id = str(uuid_mod.uuid4())
    handle = conn.stimuli.shapes.create_rect(id=client_id)
    info = conn.stimuli.query(handle)
    assert info.id == client_id
    conn.stimuli.delete(handle)
