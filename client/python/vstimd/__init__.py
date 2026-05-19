"""vstimd — Python client for vstimd.

Talks to the server over ZMQ using protobuf encoding.

Example::

    from vstimd import Connection

    with Connection() as conn:
        h = conn.stimuli.create_rect(x=-200, y=0, width=300, height=200,
                                     r=1.0, g=0.0, b=0.0)
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

from ._connection import Connection
from .system import ServerInfo, ServerVersion
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
)
from . import psychopy

__all__ = [
    "Connection",
    "ServerInfo",
    "ServerVersion",
    "VstimdError",
    "HandleNotFoundError",
    "WrongStimulusTypeError",
    "WrongTargetError",
    "CreationFailedError",
    "InvalidArgumentError",
    "NotSupportedError",
    "NotReadyError",
    "UnknownServerError",
    "psychopy",
]
