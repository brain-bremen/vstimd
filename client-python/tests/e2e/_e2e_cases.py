"""Shared e2e test cases. Imported by test_e2e.py and test_e2e_null.py.

Each function receives a `conn` fixture from the importing test module,
so the same cases run against both a real and a null-renderer server.
"""

import time

from wonderlamp import Connection


def test_create_rect(conn: Connection) -> None:
    handle = conn.stimuli.create_rect(x=0, y=0, width=100, height=100, r=1.0, g=0.0, b=0.0)
    assert handle > 0
    time.sleep(1.0)
    conn.stimuli.delete(handle)
