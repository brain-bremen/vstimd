"""E2E tests for shared stimulus mutations (shared_set_requests.proto)."""
from __future__ import annotations

import pytest

from vstimd import Connection, InvalidArgumentError, NotSupportedError
from vstimd.stimuli.stimuli_models import Color, Vec2


def test_set_enabled(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.set_enabled(handle, False)
    assert conn.stimuli.query(handle).enabled is False
    conn.stimuli.set_enabled(handle, True)
    assert conn.stimuli.query(handle).enabled is True
    conn.stimuli.delete(handle)


def test_delete(conn: Connection) -> None:
    from vstimd import HandleNotFoundError
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.delete(handle)
    with pytest.raises(HandleNotFoundError):
        conn.stimuli.query(handle)


def test_set_name(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(name="original")
    assert conn.stimuli.query(handle).name == "original"
    conn.stimuli.set_name(handle, "renamed")
    assert conn.stimuli.query(handle).name == "renamed"
    conn.stimuli.set_name(handle, "")
    assert conn.stimuli.query(handle).name == ""
    conn.stimuli.delete(handle)


def test_create_with_name(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(name="fix_cross")
    info = conn.stimuli.query(handle)
    assert info.name == "fix_cross"
    assert len(info.id) > 0
    conn.stimuli.delete(handle)


def test_create_with_invalid_uuid_fails(conn: Connection) -> None:
    with pytest.raises(InvalidArgumentError, match="valid UUID"):
        conn.stimuli.shapes.create_rect(id="not-a-uuid")


def test_set_position(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0))
    conn.stimuli.set_position(handle, Vec2(200, -100))
    info = conn.stimuli.query(handle)
    assert info.pos.x == pytest.approx(200.0, abs=0.5)
    assert info.pos.y == pytest.approx(-100.0, abs=0.5)
    conn.stimuli.delete(handle)


def test_set_orientation(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.set_orientation(handle, 30.0)
    assert conn.stimuli.query(handle).orientation == pytest.approx(30.0, abs=0.1)
    conn.stimuli.delete(handle)


def test_set_fill_color(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(color=Color(1.0, 1.0, 1.0))
    conn.stimuli.set_fill_color(handle, Color(0.2, 0.4, 0.8))
    info = conn.stimuli.query(handle)
    assert info.fill_color.r == pytest.approx(0.2, abs=0.01)
    assert info.fill_color.g == pytest.approx(0.4, abs=0.01)
    assert info.fill_color.b == pytest.approx(0.8, abs=0.01)
    conn.stimuli.delete(handle)


def test_set_alpha(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect()
    conn.stimuli.set_alpha(handle, 0.6)
    assert conn.stimuli.query(handle).opacity == pytest.approx(0.6, abs=0.01)
    conn.stimuli.delete(handle)


@pytest.mark.xfail(raises=NotSupportedError, strict=True, reason="not yet implemented")
def test_bring_to_front(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_rect()
    conn.stimuli.bring_to_front(h1)
    assert conn.stimuli.query(h1).draw_order > conn.stimuli.query(h2).draw_order
    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


@pytest.mark.xfail(raises=NotSupportedError, strict=True, reason="not yet implemented")
def test_send_to_back(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_rect()
    conn.stimuli.send_to_back(h2)
    assert conn.stimuli.query(h2).draw_order < conn.stimuli.query(h1).draw_order
    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


@pytest.mark.xfail(raises=NotSupportedError, strict=True, reason="not yet implemented")
def test_swap_draw_order(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_rect()
    order1_before = conn.stimuli.query(h1).draw_order
    order2_before = conn.stimuli.query(h2).draw_order
    conn.stimuli.swap_draw_order(h1, h2)
    assert conn.stimuli.query(h1).draw_order == order2_before
    assert conn.stimuli.query(h2).draw_order == order1_before
    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)
