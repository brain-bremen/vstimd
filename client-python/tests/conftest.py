"""Shared pytest configuration and fixtures."""

import importlib.util
import os
import sys
from unittest.mock import MagicMock

import pytest

_E2E_DEFAULT = os.environ.get("WONDERLAMP_SERVER", "tcp://localhost:5555")


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--server",
        default=_E2E_DEFAULT,
        help=f"ZMQ address of the wonderlamp_server for e2e tests (default: {_E2E_DEFAULT})",
    )

_OPTIONAL_MODULES = [
    "arabic_reshaper",
    "bidi",
    "bidi.algorithm",
    "freetype",
    "requests",
    "yaml",
]

for _mod in _OPTIONAL_MODULES:
    if _mod in sys.modules:
        continue
    try:
        available = importlib.util.find_spec(_mod) is not None
    except (ModuleNotFoundError, ValueError):
        available = False
    if not available:
        sys.modules[_mod] = MagicMock()
