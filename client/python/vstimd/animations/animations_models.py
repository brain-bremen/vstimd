from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum, IntFlag

from vstimd._handles import AnimationHandle, StimulusHandle


class AnimationState(IntEnum):
    IDLE = 0
    ARMED = 1
    RUNNING = 2
    DONE = 3


class VtlEdge(IntEnum):
    RISING = 0
    FALLING = 1


class StartAction(IntFlag):
    ENABLE                    = 0x02
    TOGGLE_PHOTODIODE         = 0x04
    START_ACTION_TRIGGER_LINE = 0x08


class FinalAction(IntFlag):
    DISABLE           = 0x01
    TOGGLE_PHOTODIODE = 0x04
    FINAL_ACTION_TRIGGER_LINE = 0x08
    RESTART                   = 0x10
    REVERSE                   = 0x20
    RESTORE_STATE             = 0x40
    END_DEFERRED              = 0x80


@dataclass(frozen=True)
class AnimationInfo:
    handle: AnimationHandle
    name: str
    state: AnimationState
    type_name: str


@dataclass(frozen=True)
class AnimationDetails:
    handle: AnimationHandle
    name: str
    state: AnimationState
    type_name: str
    stimuli: tuple[StimulusHandle, ...]
    final_action: FinalAction
