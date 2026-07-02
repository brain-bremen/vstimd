from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum


class VtlKind(IntEnum):
    # Values mirror the proto VirtualTriggerLineKind (UNSPECIFIED=0 is never a
    # valid line kind, so it is not exposed here).
    INPUT = 1
    OUTPUT = 2


@dataclass(frozen=True)
class VtlLineInfo:
    name: str
    bank: int
    bit: int
    kind: VtlKind
    high: bool
