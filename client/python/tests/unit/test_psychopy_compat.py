"""Check that vstimd.visual classes accept the same parameters, properties,
and methods as psychopy.visual."""

import inspect

import pytest

import vstimd.psychopy.visual

psychopy_visual = pytest.importorskip("psychopy.visual")

# PsychoPy params that are deprecated and intentionally not implemented.
_DEPRECATED_PARAMS: dict[str, set[str]] = {
    "GratingStim": {"rgb", "dkl", "lms"},
}

# (psychopy_class, vstimd_class, xfail_reason or None)
CLASSES = [
    (psychopy_visual.Rect,         vstimd.psychopy.visual.Rect,         None),
    (psychopy_visual.Circle,       vstimd.psychopy.visual.Circle,       None),
    (psychopy_visual.GratingStim,  vstimd.psychopy.visual.GratingStim,  None),
    (psychopy_visual.Window,       vstimd.psychopy.visual.Window,
     "vstimd.psychopy.visual.Window is a remote connection stub; "
     "rendering params not yet implemented"),
]

_PARAMS = [
    pytest.param(p_cls, w_cls, id=w_cls.__name__,
                 marks=pytest.mark.xfail(reason=reason) if reason else [])
    for p_cls, w_cls, reason in CLASSES
]


# ── helpers ──────────────────────────────────────────────────────────────────

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
    deprecated = _DEPRECATED_PARAMS.get(our_cls.__name__, set())
    missing_params   = _params(psychopy_cls)          - _params(our_cls)          - deprecated
    missing_methods  = _public_methods(psychopy_cls)  - _public_methods(our_cls)
    missing_props    = _public_props(psychopy_cls)    - _public_props(our_cls)

    if not any([missing_params, missing_methods, missing_props]):
        return None

    def fmt(label: str, names: set[str]) -> str:
        items = "\n".join(f"      {n}" for n in sorted(names))
        return f"  {label}:\n{items}"

    sections = [f"{our_cls.__name__} is missing:"]
    if missing_params:
        sections.append(fmt("__init__ params", missing_params))
    if missing_props:
        sections.append(fmt("properties", missing_props))
    if missing_methods:
        sections.append(fmt("methods", missing_methods))
    return "\n".join(sections)


# ── tests ────────────────────────────────────────────────────────────────────

@pytest.mark.parametrize("psychopy_cls,our_cls", _PARAMS)
def test_compat(psychopy_cls, our_cls):
    report = _compat_report(psychopy_cls, our_cls)
    assert report is None, report


def test_compat_summary(capsys):
    compatible = [
        w_cls.__name__
        for p_cls, w_cls, _ in CLASSES
        if _compat_report(p_cls, w_cls) is None
    ]
    with capsys.disabled():
        print(f"\nFully compatible with psychopy.visual: {compatible}")
