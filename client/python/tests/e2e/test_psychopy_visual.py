"""E2E tests for the psychopy-compatible visual API against a real vstimd server.

Skipped in CI and when no display is available.

    make test-e2e
    uv run pytest tests/e2e/test_psychopy_visual.py --server tcp://192.168.1.10:5555
"""

import os
import pathlib
import subprocess
import sys
import tempfile
import time

import pytest

import vstimd.psychopy.visual as visual

from ._psychopy_visual_cases import *  # noqa: F401, F403
from .conftest import reachable

_REPO_ROOT = pathlib.Path(__file__).parents[4]


@pytest.fixture(scope="session")
def server_address(request: pytest.FixtureRequest) -> str:
    return request.config.getoption("--server")


@pytest.fixture(scope="session", autouse=True)
def server_process(server_address: str):
    """Start the real server; skip in CI or without a display."""
    if os.environ.get("CI"):
        pytest.skip("e2e tests skipped in CI")

    has_display = (
        sys.platform == "win32"
        or os.environ.get("DISPLAY")
        or os.environ.get("WAYLAND_DISPLAY")
    )
    if not has_display:
        pytest.skip("e2e tests require a display (no DISPLAY/WAYLAND_DISPLAY set)")

    if reachable(server_address):
        yield
        return

    result = subprocess.run(["cargo", "build", "--release"], cwd=_REPO_ROOT)
    if result.returncode != 0:
        pytest.skip(f"cargo build --release failed (exit {result.returncode})")

    exe = "vstimd.exe" if sys.platform == "win32" else "vstimd"
    server_bin = _REPO_ROOT / "target" / "release" / exe
    log_path = pathlib.Path(tempfile.gettempdir()) / "vstimd_e2e.log"
    log_file = log_path.open("w")
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "debug")
    proc = subprocess.Popen(
        [str(server_bin)], stdout=log_file, stderr=log_file, env=env
    )

    for _ in range(20):
        if reachable(server_address):
            break
        time.sleep(0.5)
    else:
        proc.terminate()
        log_file.close()
        pytest.skip("server did not become ready in time")

    yield
    proc.terminate()
    proc.wait(timeout=5)
    log_file.close()
    print(f"\nServer log: {log_path}")


@pytest.fixture
def step_delay(request: pytest.FixtureRequest) -> float:
    return request.config.getoption("--step-delay")


@pytest.fixture(scope="session")
def win(server_address: str) -> visual.Window:
    w = visual.Window(address=server_address)
    yield w
    w.close()
