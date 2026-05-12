"""E2E tests for the psychopy-compatible visual API against a null-mode server.

Runs in CI and on any machine — no display or GPU required.

    make test-e2e-null
    uv run pytest tests/e2e/test_psychopy_visual_null.py
"""

import pathlib
import subprocess
import time

import pytest

import vstimd.psychopy.visual as visual

from ._psychopy_visual_cases import *  # noqa: F401, F403
from .conftest import reachable

_REPO_ROOT = pathlib.Path(__file__).parents[4]
_DEFAULT_ADDRESS = "tcp://localhost:5555"


@pytest.fixture(scope="session")
def server_address() -> str:
    return _DEFAULT_ADDRESS


@pytest.fixture(scope="session", autouse=True)
def server_process(server_address: str):
    """Build and start the server in null mode. Never skipped."""
    if reachable(server_address):
        yield
        return

    server_bin = _REPO_ROOT / "target" / "release" / "vstimd"
    if not server_bin.exists():
        result = subprocess.run(["cargo", "build", "--release"], cwd=_REPO_ROOT)
        if result.returncode != 0:
            pytest.fail(f"cargo build --release failed (exit {result.returncode})")
    if not server_bin.exists():
        pytest.fail(f"server binary not found at {server_bin}")

    proc = subprocess.Popen([str(server_bin), "--null"])

    for _ in range(20):
        if reachable(server_address):
            break
        time.sleep(0.5)
    else:
        proc.terminate()
        pytest.fail("null server did not become ready in time")

    yield
    proc.terminate()
    proc.wait(timeout=5)


@pytest.fixture
def step_delay() -> float:
    return 0.0


@pytest.fixture(scope="session")
def win(server_address: str) -> visual.Window:
    w = visual.Window(address=server_address)
    yield w
    w.close()
