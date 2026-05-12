from __future__ import annotations

from enum import StrEnum

from vstimd._proto.vstimd.v1 import stimuli_2d_pb2 as _pb2
from ._colors import normalize_color
from ._types import ColorInput, Vec2
from ._units import to_pixels
from .window import Window


class GratingTex(StrEnum):
    SIN = "sin"
    SQR = "sqr"
    SAW = "saw"
    TRI = "tri"


class GratingMask(StrEnum):
    NONE       = "none"
    CIRCLE     = "circle"
    GAUSS      = "gauss"
    RAISED_COS = "raisedCos"
    HANN       = "hann"


# tex → WaveformType proto int
_WAVEFORM_MAP: dict[GratingTex, int] = {
    GratingTex.SIN: _pb2.WAVEFORM_TYPE_SIN,
    GratingTex.SQR: _pb2.WAVEFORM_TYPE_SQR,
    GratingTex.SAW: _pb2.WAVEFORM_TYPE_SAW,
    GratingTex.TRI: _pb2.WAVEFORM_TYPE_TRI,
}

# mask → MaskType proto int
_MASK_MAP: dict[GratingMask, int] = {
    GratingMask.NONE:       _pb2.MASK_TYPE_NONE,
    GratingMask.CIRCLE:     _pb2.MASK_TYPE_CIRCLE,
    GratingMask.GAUSS:      _pb2.MASK_TYPE_GAUSS,
    GratingMask.RAISED_COS: _pb2.MASK_TYPE_RAISED_COS,
    GratingMask.HANN:       _pb2.MASK_TYPE_HANN,
}


def _parse_mask_param(mask: GratingMask | str | None, mask_params: dict | None) -> float:
    """Extract the mask-specific scalar parameter from a PsychoPy maskParams dict.

    - gauss:     {'sd': float}         — SD in normalized units (patch radius = 1)
    - raisedCos: {'fringeWidth': float} — fringe proportion [0, 1]
    Returns 0.0 (use server default) when maskParams is absent or the key is missing.
    """
    if not mask_params:
        return 0.0
    if mask == GratingMask.GAUSS:
        return float(mask_params.get("sd", 0.0))
    if mask == GratingMask.RAISED_COS:
        return float(mask_params.get("fringeWidth", 0.0))
    return 0.0


class GratingStim:
    """Grating stimulus compatible with psychopy.visual.GratingStim.

    Parameters mirror the PsychoPy API.  Parameters that vstimd does not
    support (texRes, interpolate, anchor, etc.) are silently accepted and
    ignored so that existing PsychoPy scripts need minimal changes.

    Key differences from PsychoPy:
    - ``sf`` is in cycles/pixel (same as PsychoPy units='pix').  Pass the
      window's pixel-per-degree conversion if you need cycles/degree.
    - Drift is handled server-side: set ``drift_speed`` (cycles/s) once;
      the server accumulates phase every frame, avoiding ZMQ-jitter.
    - ``phase`` is a scalar in [0, 1].  Two-element (x, y) phases are
      not supported — only the first element is used.
    """

    def __init__(
        self,
        win: Window,
        tex: GratingTex | str = GratingTex.SIN,
        mask: GratingMask | str | None = None,
        units: str = "",
        pos: Vec2 = (0.0, 0.0),
        size: Vec2 | float | None = None,
        sf: float = 0.05,
        ori: float = 0.0,
        phase: float | tuple[float, float] = 0.0,
        color: ColorInput = "white",
        colorSpace: str = "rgb",
        contrast: float = 1.0,
        opacity: float = 1.0,
        # drift (vstimd extension — not in PsychoPy)
        drift_speed: float = 0.0,
        drift_decoupled: bool = False,
        drift_angle: float = 0.0,
        autoDraw: bool = False,
        name: str | None = None,
        # accepted-but-ignored PsychoPy params
        autoLog: bool | None = None,
        texRes: int = 128,
        depth: int = 0,
        interpolate: bool = True,
        draggable: bool = False,
        anchor: str | None = None,
        blendmode: str = "avg",
        rgbPedestal: tuple[float, float, float] = (0.0, 0.0, 0.0),
        maskParams: dict | None = None,
    ) -> None:
        # ── Coerce and validate tex / mask ────────────────────────────────────
        try:
            tex = GratingTex(tex) if tex is not None else None
        except ValueError:
            raise NotImplementedError(
                f"GratingStim: tex={tex!r} is not supported. "
                f"Supported values: {[e.value for e in GratingTex]} or None."
            )
        try:
            mask = GratingMask(mask) if mask is not None else None
        except ValueError:
            raise NotImplementedError(
                f"GratingStim: mask={mask!r} is not supported. "
                f"Supported values: {[e.value for e in GratingMask]} or None."
            )
        if hasattr(phase, "__len__") and len(phase) > 1 and float(phase[1]) != 0.0:  # type: ignore[index]
            raise NotImplementedError(
                "GratingStim: two-element phase (x, y) is not supported — only the x component is used. "
                "Pass a scalar or ensure phase[1] == 0."
            )
        if blendmode != "avg":
            raise NotImplementedError(
                f"GratingStim: blendmode={blendmode!r} is not supported. Only 'avg' is implemented."
            )
        if anchor is not None and anchor != "center":
            raise NotImplementedError(
                f"GratingStim: anchor={anchor!r} is not supported. Only None/'center' is implemented."
            )
        if colorSpace not in ("rgb", "rgb255", "rgb1", ""):
            raise NotImplementedError(
                f"GratingStim: colorSpace={colorSpace!r} is not supported. "
                "Supported values: 'rgb', 'rgb255', 'rgb1'."
            )
        if draggable:
            raise NotImplementedError("GratingStim: draggable=True is not supported.")
        if tuple(rgbPedestal) != (0.0, 0.0, 0.0):
            raise NotImplementedError(
                f"GratingStim: rgbPedestal={rgbPedestal!r} is not supported. Only (0, 0, 0) is accepted."
            )

        self._win = win
        self._units = units
        self._color_space = colorSpace
        self.name = name

        # Size
        if size is None:
            width = height = 256.0
        elif isinstance(size, (int, float)):
            width = height = float(size)
        else:
            width, height = float(size[0]), float(size[1])
        self._width = width
        self._height = height

        # Phase: PsychoPy accepts (x, y) but we use only one axis.
        self._phase = float(phase[0]) if hasattr(phase, "__len__") else float(phase)  # type: ignore[index]
        self._pos: tuple[float, float] = (float(pos[0]), float(pos[1]))
        self._ori = float(ori)
        self._sf = float(sf)
        self._contrast = float(contrast)
        self._opacity = float(opacity)
        self._color: ColorInput = color
        self._drift_speed = float(drift_speed)
        self._drift_decoupled = bool(drift_decoupled)
        self._drift_angle = float(drift_angle)
        self._auto_draw = False

        waveform  = _WAVEFORM_MAP[tex]  if tex  is not None else _pb2.WAVEFORM_TYPE_SIN
        mask_type = _MASK_MAP[mask]     if mask is not None else _pb2.MASK_TYPE_NONE
        mask_param = _parse_mask_param(mask, maskParams)

        px, py = self._to_px(self._pos)
        pw = self._scalar_px(self._width)
        ph = self._scalar_px(self._height)
        # sf is in cycles/unit — convert to cycles/pixel
        psf = self._sf_to_px(self._sf)
        rgba = normalize_color(color, colorSpace, opacity) or (1.0, 1.0, 1.0, 1.0)

        self._handle: int = win._conn.stimuli.create_grating(
            x=px, y=py,
            width=pw, height=ph,
            sf=psf,
            phase=self._phase,
            angle=self._ori,
            contrast=self._contrast,
            r=rgba[0], g=rgba[1], b=rgba[2], opacity=rgba[3],
            waveform=waveform,
            mask=mask_type,
            mask_param=mask_param,
            drift_speed=self._drift_speed,
            drift_decoupled=self._drift_decoupled,
            drift_angle=self._drift_angle,
        )

        if autoDraw:
            self.autoDraw = True

    # ── Internal helpers ──────────────────────────────────────────────────────

    def _effective_units(self) -> str:
        return self._win._resolve_units(self._units)

    def _to_px(self, pos: Vec2) -> tuple[float, float]:
        result = to_pixels(pos, self._effective_units(), self._win.size, self._win.monitor)
        assert isinstance(result, tuple)
        return result

    def _scalar_px(self, val: float) -> float:
        result = to_pixels(val, self._effective_units(), self._win.size, self._win.monitor)
        assert isinstance(result, float)
        return result

    def _sf_to_px(self, sf: float) -> float:
        """Convert spatial frequency from units/cycle to cycles/pixel."""
        units = self._effective_units()
        if units in ("pix", ""):
            return sf
        # For other units, convert 1 unit to pixels then invert.
        one_unit_px = self._scalar_px(1.0)
        return sf / one_unit_px if one_unit_px != 0.0 else sf

    # ── autoDraw / draw ───────────────────────────────────────────────────────

    @property
    def autoDraw(self) -> bool:
        return self._auto_draw

    @autoDraw.setter
    def autoDraw(self, value: bool) -> None:
        self._auto_draw = bool(value)
        self._win._dispatch(self._win._conn.stimuli.set_enabled, self._handle, self._auto_draw)

    def setAutoDraw(self, value: bool, log: bool | None = None) -> None:
        self.autoDraw = value

    def draw(self) -> None:
        self._win._to_draw_once.append(self._handle)

    # ── Transform ─────────────────────────────────────────────────────────────

    @property
    def pos(self) -> tuple[float, float]:
        return self._pos

    @pos.setter
    def pos(self, value: Vec2) -> None:
        self._pos = (float(value[0]), float(value[1]))
        px, py = self._to_px(self._pos)
        self._win._dispatch(self._win._conn.stimuli.set_position, self._handle, px, py)

    def setPos(self, value: Vec2, operation: str = "", log: bool | None = None) -> None:
        if operation == "+":
            value = (self._pos[0] + value[0], self._pos[1] + value[1])
        elif operation == "-":
            value = (self._pos[0] - value[0], self._pos[1] - value[1])
        self.pos = value

    @property
    def ori(self) -> float:
        return self._ori

    @ori.setter
    def ori(self, value: float) -> None:
        self._ori = float(value)
        self._win._dispatch(self._win._conn.stimuli.set_orientation, self._handle, self._ori)

    def setOri(self, value: float, operation: str = "", log: bool | None = None) -> None:
        self.ori = value

    @property
    def size(self) -> tuple[float, float]:
        return (self._width, self._height)

    # ── Grating parameters ────────────────────────────────────────────────────

    @property
    def sf(self) -> float:
        return self._sf

    @sf.setter
    def sf(self, value: float) -> None:
        self._sf = float(value)
        psf = self._sf_to_px(self._sf)
        self._win._dispatch(self._win._conn.stimuli.set_grating_sf, self._handle, psf)

    def setSF(self, value: float, log: bool | None = None) -> None:
        self.sf = value

    @property
    def phase(self) -> float:
        return self._phase

    @phase.setter
    def phase(self, value: float | tuple[float, float]) -> None:
        self._phase = float(value[0]) if hasattr(value, "__len__") else float(value)  # type: ignore[index]
        self._win._dispatch(self._win._conn.stimuli.set_grating_phase, self._handle, self._phase)

    def setPhase(self, value: float, operation: str = "", log: bool | None = None) -> None:
        if operation == "+":
            value = self._phase + float(value)
        elif operation == "-":
            value = self._phase - float(value)
        self.phase = value

    @property
    def contrast(self) -> float:
        return self._contrast

    @contrast.setter
    def contrast(self, value: float) -> None:
        self._contrast = float(value)
        self._win._dispatch(self._win._conn.stimuli.set_grating_contrast, self._handle, self._contrast)

    def setContrast(self, value: float, log: bool | None = None) -> None:
        self.contrast = value

    @property
    def opacity(self) -> float:
        return self._opacity

    @opacity.setter
    def opacity(self, value: float) -> None:
        self._opacity = float(value)
        self._resend_color()

    def setOpacity(self, value: float, log: bool | None = None) -> None:
        self.opacity = value

    @property
    def color(self) -> ColorInput:
        return self._color

    @color.setter
    def color(self, value: ColorInput) -> None:
        self._color = value
        self._resend_color()

    def setColor(self, value: ColorInput, colorSpace: str | None = None, log: bool | None = None) -> None:
        if colorSpace is not None:
            if colorSpace not in ("rgb", "rgb255", "rgb1", ""):
                raise NotImplementedError(
                    f"GratingStim: colorSpace={colorSpace!r} is not supported. "
                    "Supported values: 'rgb', 'rgb255', 'rgb1'."
                )
            self._color_space = colorSpace
        self.color = value

    def _resend_color(self) -> None:
        rgba = normalize_color(self._color, self._color_space, self._opacity) or (1.0, 1.0, 1.0, 1.0)
        self._win._dispatch(
            self._win._conn.stimuli.set_fill_color,
            self._handle, rgba[0], rgba[1], rgba[2], rgba[3],
        )

    # ── Drift (vstimd extension) ──────────────────────────────────────────────

    @property
    def drift_speed(self) -> float:
        return self._drift_speed

    @drift_speed.setter
    def drift_speed(self, value: float) -> None:
        self._drift_speed = float(value)
        self._win._dispatch(
            self._win._conn.stimuli.set_grating_drift_speed, self._handle, self._drift_speed
        )

    @property
    def drift_decoupled(self) -> bool:
        return self._drift_decoupled

    @drift_decoupled.setter
    def drift_decoupled(self, value: bool) -> None:
        self._drift_decoupled = bool(value)
        self._win._dispatch(
            self._win._conn.stimuli.set_grating_drift_decoupled, self._handle, self._drift_decoupled
        )

    @property
    def drift_angle(self) -> float:
        return self._drift_angle

    @drift_angle.setter
    def drift_angle(self, value: float) -> None:
        self._drift_angle = float(value)
        self._win._dispatch(
            self._win._conn.stimuli.set_grating_drift_angle, self._handle, self._drift_angle
        )
