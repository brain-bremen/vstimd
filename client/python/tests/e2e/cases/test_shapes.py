"""E2E tests for shape stimuli."""
from __future__ import annotations

import time
import uuid as uuid_mod

import pytest

from vstimd import Connection, InvalidArgumentError
from vstimd.stimuli import RectParams, StimulusType
from vstimd.stimuli.stimuli_models import Color, Vec2
from ._helpers import label as _label


def test_create_rect(conn: Connection, request: pytest.FixtureRequest) -> None:
    tid = request.node.name
    lbl = _label(conn, tid, "red 100×100 rect")
    handle = conn.stimuli.shapes.create_rect(pos=Vec2(0, 0), width=100, height=100, color=Color(1.0, 0.0, 0.0))
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
    conn.stimuli.delete(lbl)


def test_create_with_name(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=100, height=100, name="fix_cross")
    assert handle > 0
    info = conn.stimuli.query(handle)
    assert info.name == "fix_cross"
    assert len(info.id) > 0
    conn.stimuli.delete(handle)


def test_create_with_client_uuid(conn: Connection) -> None:
    client_id = str(uuid_mod.uuid4())
    handle = conn.stimuli.shapes.create_rect(width=100, height=100, id=client_id)
    assert handle > 0
    info = conn.stimuli.query(handle)
    assert info.id == client_id
    conn.stimuli.delete(handle)


def test_create_with_invalid_client_uuid_fails(conn: Connection) -> None:
    with pytest.raises(InvalidArgumentError, match="valid UUID"):
        conn.stimuli.shapes.create_rect(width=100, height=100, id="not-a-uuid")


def test_set_name(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=100, height=100, name="original")
    conn.stimuli.set_name(handle, "renamed")
    info = conn.stimuli.query(handle)
    assert info.name == "renamed"
    conn.stimuli.delete(handle)


def test_list_stimuli_includes_id_and_name(conn: Connection) -> None:
    from vstimd._proto import service_pb2, system_pb2
    h1 = conn.stimuli.shapes.create_rect(width=50, height=50, name="stim_a")
    h2 = conn.stimuli.shapes.create_circle(radius=30, name="stim_b")

    resp = conn._send(service_pb2.Request(
        system=service_pb2.SystemTarget(),
        list_stimuli=system_pb2.ListStimuliRequest(),
    ))
    entries = {e.handle: e for e in resp.stimulus_list.entries}

    assert h1 in entries
    assert h2 in entries
    assert entries[h1].name == "stim_a"
    assert entries[h2].name == "stim_b"
    assert len(entries[h1].id) > 0
    assert len(entries[h2].id) > 0

    conn.stimuli.delete(h1)
    conn.stimuli.delete(h2)


def test_uuid_stable_across_query(conn: Connection) -> None:
    handle = conn.stimuli.shapes.create_rect(width=80, height=80)
    id1 = conn.stimuli.query(handle).id
    id2 = conn.stimuli.query(handle).id
    assert id1 == id2
    assert len(id1) > 0
    conn.stimuli.delete(handle)
