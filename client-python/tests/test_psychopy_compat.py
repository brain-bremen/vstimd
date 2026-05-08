"""Check that wonderlamp.visual classes accept the same parameters, properties,
and methods as psychopy.visual."""

import inspect

import pytest

import wonderlamp.visual

psychopy_visual = pytest.importorskip("psychopy.visual")


def _params(cls: type) -> set[str]:
    return set(inspect.signature(cls.__init__).parameters) - {"self"}


def _public_methods(cls: type) -> set[str]:
    result = set()
    for name in dir(cls):
        if name.startswith("_"):
            continue
        for klass in cls.__mro__:
            if name in vars(klass):
                v = vars(klass)[name]
                if callable(v) and not isinstance(v, property):
                    result.add(name)
                break
    return result


def _public_props(cls: type) -> set[str]:
    result = set()
    for name in dir(cls):
        if name.startswith("_"):
            continue
        for klass in cls.__mro__:
            if name in vars(klass):
                if isinstance(vars(klass)[name], property):
                    result.add(name)
                break
    return result


def _compat_report(psychopy_cls: type, our_cls: type) -> str | None:
    missing_params = _params(psychopy_cls) - _params(our_cls)
    missing_methods = _public_methods(psychopy_cls) - _public_methods(our_cls)
    missing_props = _public_props(psychopy_cls) - _public_props(our_cls)

    if not any([missing_params, missing_methods, missing_props]):
        return None

    lines = [f"{our_cls.__name__} is missing:"]
    if missing_params:
        lines.append(f"  __init__ params : {sorted(missing_params)}")
    if missing_props:
        lines.append(f"  properties      : {sorted(missing_props)}")
    if missing_methods:
        lines.append(f"  methods         : {sorted(missing_methods)}")
    return "\n".join(lines)


def test_rect_compat():
    report = _compat_report(psychopy_visual.Rect, wonderlamp.visual.Rect)
    assert report is None, report


def test_circle_compat():
    report = _compat_report(psychopy_visual.Circle, wonderlamp.visual.Circle)
    assert report is None, report


@pytest.mark.xfail(reason="wonderlamp.visual.Window is a remote connection stub; "
                           "rendering params not yet implemented")
def test_window_compat():
    report = _compat_report(psychopy_visual.Window, wonderlamp.visual.Window)
    assert report is None, report
