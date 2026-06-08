from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum, IntFlag


class AnimationState(IntEnum):
    IDLE = 0
    ARMED = 1
    RUNNING = 2
    DONE = 3


class VtlEdge(IntEnum):
    RISING = 0
    FALLING = 1


class FinalAction(IntFlag):
    DISABLE           = 0x01
    TOGGLE_PHOTODIODE = 0x04
    FINAL_ACTION_TRIGGER_LINE = 0x08
    RESTART           = 0x10
    REVERSE           = 0x20
    END_DEFERRED      = 0x80


@dataclass(frozen=True)
class AnimationInfo:
    handle: int
    name: str
    state: AnimationState
    type_name: str


@dataclass(frozen=True)
class AnimationDetails:
    handle: int
    name: str
    state: AnimationState
    type_name: str
    stimuli: tuple[int, ...]
    final_action: FinalAction
