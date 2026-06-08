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


class AnimatedParam(IntEnum):
    POSITION_X = 0
    POSITION_Y = 1
    ALPHA = 2
    GRATING_PHASE = 3
    GRATING_CONTRAST = 4
    GRATING_SF = 5


class FinalAction(IntFlag):
    DISABLE           = 0x01
    TOGGLE_PHOTODIODE = 0x04
    SIGNAL_EVENT      = 0x08
    RESTART           = 0x10
    REVERSE           = 0x20
    END_DEFERRED      = 0x80


@dataclass(frozen=True)
class AnimationInfo:
    handle: int
    name: str
    state: AnimationState
    type_name: str
