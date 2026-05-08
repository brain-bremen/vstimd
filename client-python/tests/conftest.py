"""Mock optional psychopy dependencies that are not needed for signature inspection."""

import importlib.util
import sys
from unittest.mock import MagicMock

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
