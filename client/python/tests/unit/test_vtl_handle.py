"""Unit tests for the VtlHandle address type (no server required)."""

from __future__ import annotations

import pytest

from vstimd.vtl import VtlDirection, VtlHandle


def test_input_output_constructors() -> None:
    h = VtlHandle.input(0, 10)
    assert (h.direction, h.bank, h.bit, h.name) == (VtlDirection.INPUT, 0, 10, None)

    h = VtlHandle.output(1, 3)
    assert (h.direction, h.bank, h.bit, h.name) == (VtlDirection.OUTPUT, 1, 3, None)


def test_named_requires_explicit_direction() -> None:
    h = VtlHandle.named("frame_sync", VtlDirection.OUTPUT)
    assert (h.direction, h.name, h.bank, h.bit) == (VtlDirection.OUTPUT, "frame_sync", None, None)


def test_empty_handle_is_rejected() -> None:
    # No name and no (bank, bit) — not a usable address.
    with pytest.raises(ValueError):
        VtlHandle(VtlDirection.INPUT)


def test_partial_bank_bit_is_rejected() -> None:
    with pytest.raises(ValueError):
        VtlHandle(VtlDirection.INPUT, bank=0)  # missing bit
    with pytest.raises(ValueError):
        VtlHandle(VtlDirection.OUTPUT, bit=5)  # missing bank


def test_name_and_bank_bit_are_mutually_exclusive() -> None:
    with pytest.raises(ValueError):
        VtlHandle(VtlDirection.INPUT, bank=0, bit=1, name="both")
