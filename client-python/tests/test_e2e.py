"""End-to-end tests against a real wonderlamp_server.

On a desktop (DISPLAY or WAYLAND_DISPLAY set) the server is started
automatically. In CI (CI env var set) the tests are skipped.

    make test-e2e
    uv run pytest tests/test_e2e.py --server tcp://localhost:5555
"""

import os
import pathlib
import subprocess
import time

import pytest
import zmq

from wonderlamp import Connection
from wonderlamp._proto import wonderlamp_pb2 as pb

_REPO_ROOT = pathlib.Path(__file__).parents[2]
_DEFAULT_ADDRESS = "tcp://localhost:5555"



def _reachable(address: str, timeout_ms: int = 500) -> bool:
    ctx = zmq.Context.instance()
    sock = ctx.socket(zmq.REQ)
    sock.setsockopt(zmq.LINGER, 0)
    sock.setsockopt(zmq.RCVTIMEO, timeout_ms)
    sock.connect(address)
    try:
        sock.send(pb.Request(handle=0).SerializeToString())
        sock.recv()
        return True
    except zmq.Again:
        return False
    finally:
        sock.close()


@pytest.fixture(scope="session")
def server_address(request: pytest.FixtureRequest) -> str:
    return request.config.getoption("--server")


@pytest.fixture(scope="session", autouse=True)
def server_process(server_address: str):
    """Start the server on desktop; skip the session in CI or without a display."""
    if os.environ.get("CI"):
        pytest.skip("e2e tests skipped in CI")

    has_display = os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")
    if not has_display:
        pytest.skip("e2e tests require a display (no DISPLAY/WAYLAND_DISPLAY set)")

    # If a server is already running, use it as-is.
    if _reachable(server_address):
        yield
        return

    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=_REPO_ROOT,
    )
    if result.returncode != 0:
        pytest.skip(f"cargo build --release failed (exit {result.returncode})")

    server_bin = _REPO_ROOT / "target" / "release" / "wonderlamp_server"
    proc = subprocess.Popen([str(server_bin)])

    for _ in range(20):
        if _reachable(server_address):
            break
        time.sleep(0.5)
    else:
        proc.terminate()
        pytest.skip("server did not become ready in time")

    yield
    proc.terminate()
    proc.wait(timeout=5)


@pytest.fixture(scope="session")
def conn(server_address: str) -> Connection:
    c = Connection(server_address)
    yield c
    c.close()


# ── tests ────────────────────────────────────────────────────────────────────

def test_create_rect(conn: Connection) -> None:
    handle = conn.create_rect(x=0, y=0, width=100, height=100, r=1.0, g=0.0, b=0.0)
    assert handle > 0
    time.sleep(1.0)
    conn.delete(handle)
