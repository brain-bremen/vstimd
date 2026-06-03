"""API surface tests for TextBox2 — no server or PsychoPy required."""
import inspect
import pytest
from vstimd.psychopy.visual.text import TextBox2


def _params(cls: type) -> set[str]:
    return set(inspect.signature(cls.__init__).parameters) - {"self"}


def _public_props(cls: type) -> set[str]:
    return {
        name for name in dir(cls)
        if not name.startswith("_")
        and any(isinstance(vars(k).get(name), property) for k in cls.__mro__)
    }


def _public_methods(cls: type) -> set[str]:
    return {
        name for name in dir(cls)
        if not name.startswith("_")
        and callable(getattr(cls, name, None))
        and not isinstance(inspect.getattr_static(cls, name, None), property)
    }


REQUIRED_PARAMS = {
    "win", "text", "font", "pos", "units", "letterHeight", "size",
    "color", "colorSpace", "fillColor", "opacity", "anchor",
    "languageStyle", "autoDraw", "name",
}

REQUIRED_PROPS = {"text", "color", "opacity", "pos", "autoDraw"}

REQUIRED_METHODS = {
    "draw", "setAutoDraw", "setText", "setColor", "setOpacity", "setPos",
}


def test_init_params():
    missing = REQUIRED_PARAMS - _params(TextBox2)
    assert not missing, f"TextBox2.__init__ missing params: {sorted(missing)}"


def test_properties():
    missing = REQUIRED_PROPS - _public_props(TextBox2)
    assert not missing, f"TextBox2 missing properties: {sorted(missing)}"


def test_methods():
    missing = REQUIRED_METHODS - _public_methods(TextBox2)
    assert not missing, f"TextBox2 missing methods: {sorted(missing)}"


def test_exported_from_visual():
    import vstimd.psychopy.visual as visual
    assert hasattr(visual, "TextBox2")
    assert visual.TextBox2 is TextBox2
