from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum


class VtlDirection(IntEnum):
    INPUT = 0
    OUTPUT = 1


@dataclass(frozen=True)
class VtlLineInfo:
    name: str
    bank: int
    bit: int
    direction: VtlDirection
    high: bool
