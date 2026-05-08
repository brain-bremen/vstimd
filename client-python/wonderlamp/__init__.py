"""wonderlamp — Python client for wonderlamp_server.

Talks to the server over ZMQ using protobuf encoding.

Example::

    from wonderlamp import Connection

    with Connection() as conn:
        handle = conn.create_rect(x=-200, y=0, width=300, height=200,
                                  r=1.0, g=0.0, b=0.0)
        conn.set_enabled(handle, False)
        conn.delete(handle)
"""

from ._connection import Connection
from . import psychopy

__all__ = ["Connection", "psychopy"]

