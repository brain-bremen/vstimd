"""E2E tests against a real wonderlamp_server with visible rendering.

Skipped in CI and when no display is available.

    make test-e2e
    uv run pytest tests/e2e/test_e2e.py --server tcp://192.168.1.10:5555
"""

import os
import pathlib
import subprocess
import time

import pytest

from wonderlamp import Connection
from ._e2e_cases import *  # noqa: F401, F403
from .conftest import reachable

_REPO_ROOT = pathlib.Path(__file__).parents[3]


@pytest.fixture(scope="session")
def server_address(request: pytest.FixtureRequest) -> str:
    return request.config.getoption("--server")


@pytest.fixture(scope="session", autouse=True)
def server_process(server_address: str):
    """Start the real server; skip in CI or without a display."""
    if os.environ.get("CI"):
        pytest.skip("e2e tests skipped in CI")

    has_display = os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")
    if not has_display:
        pytest.skip("e2e tests require a display (no DISPLAY/WAYLAND_DISPLAY set)")

    if reachable(server_address):
        yield
        return

    result = subprocess.run(["cargo", "build", "--release"], cwd=_REPO_ROOT)
    if result.returncode != 0:
        pytest.skip(f"cargo build --release failed (exit {result.returncode})")

    server_bin = _REPO_ROOT / "target" / "release" / "wonderlamp_server"
    proc = subprocess.Popen([str(server_bin)])

    for _ in range(20):
        if reachable(server_address):
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
