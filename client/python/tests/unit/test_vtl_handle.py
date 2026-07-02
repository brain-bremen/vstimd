"""Unit tests for the VtlHandle address type (no server required)."""

from __future__ import annotations

import pytest

from vstimd.vtl import VtlKind, VtlHandle


def test_input_output_constructors() -> None:
    h = VtlHandle.input(0, 10)
    assert (h.kind, h.bank, h.bit, h.name) == (VtlKind.INPUT, 0, 10, None)

    h = VtlHandle.output(1, 3)
    assert (h.kind, h.bank, h.bit, h.name) == (VtlKind.OUTPUT, 1, 3, None)


def test_named_requires_explicit_kind() -> None:
    h = VtlHandle.named("frame_sync", VtlKind.OUTPUT)
    assert (h.kind, h.name, h.bank, h.bit) == (VtlKind.OUTPUT, "frame_sync", None, None)


def test_empty_handle_is_rejected() -> None:
    # No name and no (bank, bit) — not a usable address.
    with pytest.raises(ValueError):
        VtlHandle(VtlKind.INPUT)


def test_partial_bank_bit_is_rejected() -> None:
    with pytest.raises(ValueError):
        VtlHandle(VtlKind.INPUT, bank=0)  # missing bit
    with pytest.raises(ValueError):
        VtlHandle(VtlKind.OUTPUT, bit=5)  # missing bank


def test_name_and_bank_bit_are_mutually_exclusive() -> None:
    with pytest.raises(ValueError):
        VtlHandle(VtlKind.INPUT, bank=0, bit=1, name="both")
