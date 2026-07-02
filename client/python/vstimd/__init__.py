"""vstimd — Python client for vstimd.

Talks to the server over ZMQ using protobuf encoding.

Example::

    from vstimd import Connection

    with Connection() as conn:
        h = conn.stimuli.shapes.create_rect(pos=Vec2(-200, 0), width=300, height=200,
                                            color=Color(1.0, 0.0, 0.0))
        conn.stimuli.set_enabled(h, False)
        conn.stimuli.delete(h)
        info = conn.system.query_server_info()
        print(info.version)
"""

# Extend the package search path so that `from vstimd.v1 import ...`
# in the generated proto stubs resolves to _proto/vstimd/v1/ without
# shadowing this package's own namespace.
import os as _os
__path__ = list(__path__) + [_os.path.join(_os.path.dirname(__file__), "_proto", "vstimd")]

from .connection import Connection
from ._handles import AnimationHandle, StimulusHandle
from .response import ErrorCode, ServerResponse
from .system import ServerInfo, ServerVersion, StimulusListEntry
from .vtl import VtlClient, VtlDirection, VtlLineInfo
from .config import ConfigClient
from .animations import (
    AnimationClient,
    AnimationDetails,
    AnimationInfo,
    AnimationState,
    CancelAction,
    FinalAction,
    StartAction,
    VtlEdge,
)
from .exceptions import (
    VstimdError,
    HandleNotFoundError,
    WrongStimulusTypeError,
    WrongTargetError,
    CreationFailedError,
    InvalidArgumentError,
    NotSupportedError,
    NotReadyError,
    UnknownServerError,
    ConfigNotFoundError,
    ConfigIoError,
    ConfigFormatError,
    ConfigVersionError,
    ConfigAlreadyExistsError,
)
from . import psychopy

__all__ = [
    "Connection",
    "AnimationHandle",
    "StimulusHandle",
    "ErrorCode",
    "ServerResponse",
    "ServerInfo",
    "ServerVersion",
    "StimulusListEntry",
    "ConfigClient",
    "ConfigNotFoundError",
    "ConfigIoError",
    "ConfigFormatError",
    "ConfigVersionError",
    "ConfigAlreadyExistsError",
    "VstimdError",
    "HandleNotFoundError",
    "WrongStimulusTypeError",
    "WrongTargetError",
    "CreationFailedError",
    "InvalidArgumentError",
    "NotSupportedError",
    "NotReadyError",
    "UnknownServerError",
    "VtlClient",
    "VtlDirection",
    "VtlLineInfo",
    "AnimationClient",
    "AnimationDetails",
    "AnimationInfo",
    "AnimationState",
    "CancelAction",
    "FinalAction",
    "StartAction",
    "VtlEdge",
    "psychopy",
]
