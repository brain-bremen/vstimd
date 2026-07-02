"""E2E tests for Virtual Trigger Line (VTL) commands."""

from __future__ import annotations

from vstimd import Connection
from vstimd.response import ErrorCode, ServerResponse
from vstimd.vtl import VtlDirection, VtlHandle


def test_vtl_set_and_list_line_name(conn: Connection) -> None:
    """Named output lines appear in list_lines with the right metadata."""
    resp = conn.vtl.set_line_name(
        bank=0, bit=0, direction=VtlDirection.OUTPUT, name="stim_onset"
    )
    assert isinstance(resp, ServerResponse)
    assert resp.code == ErrorCode.OK
    conn.vtl.set_line_name(
        bank=0, bit=1, direction=VtlDirection.OUTPUT, name="stim_offset"
    )

    lines = conn.vtl.list_lines()
    names = {l.name for l in lines}
    assert "stim_onset" in names
    assert "stim_offset" in names

    for line in lines:
        if line.name in ("stim_onset", "stim_offset"):
            assert line.bank == 0
            assert line.direction == VtlDirection.OUTPUT

    conn.vtl.set_line_name(bank=0, bit=0, direction=VtlDirection.OUTPUT, name="")
    conn.vtl.set_line_name(bank=0, bit=1, direction=VtlDirection.OUTPUT, name="")


def test_vtl_set_line_by_bank_bit(conn: Connection) -> None:
    """set_line on an INPUT handle writes the input bank; list_lines reports high."""
    conn.vtl.set_line_name(bank=0, bit=2, direction=VtlDirection.INPUT, name="test_in")
    try:
        conn.vtl.set_line(VtlHandle.input(0, 2), True)
        lines = conn.vtl.list_lines()
        info = next(l for l in lines if l.name == "test_in")
        assert info.high is True

        conn.vtl.set_line(VtlHandle.input(0, 2), False)
        lines = conn.vtl.list_lines()
        info = next(l for l in lines if l.name == "test_in")
        assert info.high is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=2, direction=VtlDirection.INPUT, name="")


def test_vtl_set_line_by_name(conn: Connection) -> None:
    """set_line accepts a named INPUT handle."""
    conn.vtl.set_line_name(bank=0, bit=3, direction=VtlDirection.INPUT, name="named_in")
    try:
        conn.vtl.set_line(VtlHandle.named("named_in", VtlDirection.INPUT), True)
        lines = conn.vtl.list_lines()
        info = next(l for l in lines if l.name == "named_in")
        assert info.high is True
    finally:
        conn.vtl.set_line_name(bank=0, bit=3, direction=VtlDirection.INPUT, name="")


def test_vtl_toggle_line(conn: Connection) -> None:
    """toggle_line flips the line and returns the new state."""
    conn.vtl.set_line_name(
        bank=0, bit=4, direction=VtlDirection.INPUT, name="toggle_in"
    )
    try:
        conn.vtl.set_line(VtlHandle.input(0, 4), False)

        new_state = conn.vtl.toggle_line(VtlHandle.input(0, 4))
        assert new_state is True

        new_state = conn.vtl.toggle_line(VtlHandle.named("toggle_in", VtlDirection.INPUT))
        assert new_state is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=4, direction=VtlDirection.INPUT, name="")


def test_vtl_set_bank(conn: Connection) -> None:
    """set_bank writes a full 64-bit word; INPUT-named bits within the bank reflect it."""
    conn.vtl.set_line_name(
        bank=0, bit=5, direction=VtlDirection.INPUT, name="bank_bit5"
    )
    conn.vtl.set_line_name(
        bank=0, bit=6, direction=VtlDirection.INPUT, name="bank_bit6"
    )
    try:
        conn.vtl.set_bank(VtlDirection.INPUT, 0, (1 << 5) | (1 << 6))
        lines = conn.vtl.list_lines()
        by_name = {l.name: l for l in lines}
        assert by_name["bank_bit5"].high is True
        assert by_name["bank_bit6"].high is True

        conn.vtl.set_bank(VtlDirection.INPUT, 0, 0)
        lines = conn.vtl.list_lines()
        by_name = {l.name: l for l in lines}
        assert by_name["bank_bit5"].high is False
        assert by_name["bank_bit6"].high is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=5, direction=VtlDirection.INPUT, name="")
        conn.vtl.set_line_name(bank=0, bit=6, direction=VtlDirection.INPUT, name="")


def test_vtl_clear_input_latches(conn: Connection) -> None:
    """clear_latches returns OK and drains accumulated input edge latches."""
    conn.vtl.set_line_name(
        bank=0, bit=7, direction=VtlDirection.INPUT, name="latch_test"
    )
    try:
        conn.vtl.set_line(VtlHandle.input(0, 7), True)
        conn.vtl.set_line(VtlHandle.input(0, 7), False)

        resp = conn.vtl.clear_latches(VtlHandle.input(0, 7))
        assert isinstance(resp, ServerResponse)
        assert resp.code == ErrorCode.OK
    finally:
        conn.vtl.set_line_name(bank=0, bit=7, direction=VtlDirection.INPUT, name="")


def test_vtl_set_output_line(conn: Connection) -> None:
    conn.vtl.set_line_name(
        bank=0, bit=10, direction=VtlDirection.OUTPUT, name="out_line"
    )
    try:
        conn.vtl.set_line(VtlHandle.output(0, 10), True)
        lines = conn.vtl.list_lines()
        info = next(l for l in lines if l.name == "out_line")
        assert info.high is True

        conn.vtl.set_line(VtlHandle.output(0, 10), False)
        lines = conn.vtl.list_lines()
        info = next(l for l in lines if l.name == "out_line")
        assert info.high is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=10, direction=VtlDirection.OUTPUT, name="")


def test_vtl_toggle_output_line(conn: Connection) -> None:
    conn.vtl.set_line_name(
        bank=0, bit=11, direction=VtlDirection.OUTPUT, name="out_toggle"
    )
    try:
        conn.vtl.set_line(VtlHandle.output(0, 11), False)

        new_state = conn.vtl.toggle_line(VtlHandle.output(0, 11))
        assert new_state is True

        new_state = conn.vtl.toggle_line(VtlHandle.named("out_toggle", VtlDirection.OUTPUT))
        assert new_state is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=11, direction=VtlDirection.OUTPUT, name="")


def test_vtl_set_output_bank(conn: Connection) -> None:
    conn.vtl.set_line_name(
        bank=0, bit=12, direction=VtlDirection.OUTPUT, name="out_bank12"
    )
    conn.vtl.set_line_name(
        bank=0, bit=13, direction=VtlDirection.OUTPUT, name="out_bank13"
    )
    try:
        conn.vtl.set_bank(VtlDirection.OUTPUT, 0, (1 << 12) | (1 << 13))
        lines = conn.vtl.list_lines()
        by_name = {l.name: l for l in lines}
        assert by_name["out_bank12"].high is True
        assert by_name["out_bank13"].high is True

        conn.vtl.set_bank(VtlDirection.OUTPUT, 0, 0)
        lines = conn.vtl.list_lines()
        by_name = {l.name: l for l in lines}
        assert by_name["out_bank12"].high is False
        assert by_name["out_bank13"].high is False
    finally:
        conn.vtl.set_line_name(bank=0, bit=12, direction=VtlDirection.OUTPUT, name="")
        conn.vtl.set_line_name(bank=0, bit=13, direction=VtlDirection.OUTPUT, name="")
