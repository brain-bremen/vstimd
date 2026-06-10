"""E2E tests for scene-wide system commands (system.proto)."""
from __future__ import annotations

import pytest

from vstimd import Connection
from vstimd.response import ErrorCode, ServerResponse
from vstimd.stimuli.stimuli_models import Vec2


def test_query_server_info(conn: Connection) -> None:
    info = conn.system.query_server_info()
    assert info.width >= 0
    assert info.height >= 0
    assert info.frame_rate > 0.0
    assert info.version.major >= 0


def test_set_background(conn: Connection) -> None:
    conn.system.set_background(r=0.2, g=0.4, b=0.6)
    info = conn.system.query_server_info()
    assert info.background_color.r == pytest.approx(0.2, abs=0.01)
    assert info.background_color.g == pytest.approx(0.4, abs=0.01)
    assert info.background_color.b == pytest.approx(0.6, abs=0.01)
    conn.system.set_background(r=0.0, g=0.0, b=0.0)


def test_list_stimuli(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect(name="stim_a")
    h2 = conn.stimuli.shapes.create_circle(name="stim_b")

    entries = {e.handle: e for e in conn.system.list_stimuli()}

    assert h1 in entries and h2 in entries
    assert entries[h1].name == "stim_a"
    assert entries[h2].name == "stim_b"
    assert len(entries[h1].id) > 0

    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


def test_delete_all(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_circle()
    conn.system.delete_all()

    handles = {e.handle for e in conn.system.list_stimuli()}
    assert h1 not in handles
    assert h2 not in handles


def test_set_all_enabled(conn: Connection) -> None:
    h1 = conn.stimuli.shapes.create_rect()
    h2 = conn.stimuli.shapes.create_circle()

    conn.system.set_all_enabled(False)
    assert conn.stimuli.query(h1).enabled is False
    assert conn.stimuli.query(h2).enabled is False

    conn.system.set_all_enabled(True)
    assert conn.stimuli.query(h1).enabled is True
    assert conn.stimuli.query(h2).enabled is True

    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


def test_server_response_fields(conn: Connection) -> None:
    """Every mutation returns a ServerResponse with sensible metadata."""
    resp = conn.system.delete_all()
    assert isinstance(resp, ServerResponse)
    assert resp.code == ErrorCode.OK
    assert resp.error == ""
    assert resp.frame_count >= 0
    assert resp.server_time_ns > 0

    # frame_count must advance across successive RPCs
    r1 = conn.system.wait_for_frames(1)
    r2 = conn.system.wait_for_frames(1)
    assert r2.frame_count > r1.frame_count


def test_wait_until(conn: Connection) -> None:
    r1 = conn.system.wait_for_frames(1)
    r2 = conn.system.wait_until(r1.server_time_ns)
    assert r2.code == ErrorCode.OK


def test_wait_until_ready_already_running(server_address: str) -> None:
    """wait_until_ready returns immediately when the server is already up."""
    with Connection(server_address) as c:
        c.wait_until_ready(timeout_s=5.0)


def test_wait_ready_constructor_flag(server_address: str) -> None:
    """Connection(wait_ready=True) connects and becomes ready without extra calls."""
    with Connection(server_address, wait_ready=True, ready_timeout_s=5.0) as c:
        info = c.system.query_server_info()
        assert info.frame_rate > 0.0


def test_wait_until_ready_timeout() -> None:
    """wait_until_ready raises TimeoutError when nothing is listening."""
    with Connection("tcp://localhost:19876") as c:
        with pytest.raises(TimeoutError):
            c.wait_until_ready(timeout_s=1.0, retry_interval_s=0.2)


def test_set_deferred_mode(conn: Connection) -> None:
    h = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0))
    conn.system.set_deferred_mode(True)
    conn.stimuli.set_position(h, Vec2(100, 50))
    conn.system.set_deferred_mode(False)
    conn.system.wait_for_frames(1)
    info = conn.stimuli.query(h)
    assert info.pos.x == pytest.approx(100.0, abs=0.5)
    conn.stimuli.delete(h)
