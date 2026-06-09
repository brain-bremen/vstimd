"""PsychoPy-compatible TextBox2 stimulus."""
from __future__ import annotations

from ..._handles import StimulusHandle
from ...stimuli.stimuli_models import Color as StimulusColor, LanguageStyle, Vec2 as StimulusVec2
from ._colors import normalize_color
from ._types import ColorInput, Vec2
from ._units import to_pixels
from .window import Window


class TextBox2:
    """Multi-line text stimulus matching the PsychoPy ``TextBox2`` API.

    The text is laid out inside a bounding box with word-wrap.  The box is
    positioned at ``pos`` using the coordinate system set by ``units``.

    PsychoPy ``TextBox2`` parameters supported:

    * ``text``, ``font``, ``letterHeight``, ``size``, ``pos``, ``anchor``
    * ``color`` / ``colorSpace`` — text foreground colour
    * ``fillColor`` — background rectangle fill (transparent by default)
    * ``opacity``
    * ``autoDraw``, ``draw()``, ``setAutoDraw()``
    * ``languageStyle`` — ``"LTR"`` (default), ``"RTL"``, ``"Arabic"``

    Parameters silently accepted but not forwarded to the server (matching
    PsychoPy's own ignored-in-some-backends pattern):

    * ``borderColor``, ``borderWidth``, ``padding``, ``alignment``
    * ``bold``, ``italic``, ``lineSpacing``, ``editable``
    * ``autoLog``, ``depth``, ``draggable``
    """

    _LANGUAGE_STYLE_MAP: dict[str, LanguageStyle] = {
        "ltr":    LanguageStyle.LTR,
        "rtl":    LanguageStyle.RTL,
        "arabic": LanguageStyle.ARABIC,
    }

    def __init__(
        self,
        win: Window,
        text: str = "",
        font: str = "",
        pos: Vec2 = (0.0, 0.0),
        units: str = "",
        letterHeight: float | None = None,
        size: Vec2 | None = None,
        color: ColorInput = "white",
        colorSpace: str = "rgb",
        fillColor: ColorInput = None,
        fillColorSpace: str = "rgb",
        borderColor: ColorInput = None,
        borderWidth: float = 2.0,
        opacity: float = 1.0,
        anchor: str = "center",
        alignment: str = "left",
        languageStyle: str = "LTR",
        autoDraw: bool = False,
        name: str | None = None,
        # silently accepted
        bold: bool = False,
        italic: bool = False,
        lineSpacing: float = 1.0,
        padding: float | None = None,
        editable: bool = False,
        autoLog: bool | None = None,
        depth: int = 0,
        draggable: bool = False,
    ) -> None:
        self._win = win
        self._units = units
        self._color_space = colorSpace
        self._fill_color_space = fillColorSpace
        self.name = name

        self._pos: tuple[float, float] = (float(pos[0]), float(pos[1]))
        self._text = text
        self._color: ColorInput = color
        self._fill_color: ColorInput = fillColor
        self._opacity = float(opacity)
        self._anchor = anchor
        self._auto_draw = False

        # Letter height: PsychoPy default is ~32 px (in pix units).
        # When units are not pix, convert via the scalar path.
        if letterHeight is None:
            self._letter_height_px = 32.0
        else:
            self._letter_height_px = float(
                to_pixels(float(letterHeight), self._effective_units(), win.size, win.monitor)
            )

        # Box size in pixels.
        if size is None:
            sw, sh = win.size
            self._box_w = float(sw) * 0.5
            self._box_h = float(sh) * 0.25
        else:
            bw = to_pixels(float(size[0]), self._effective_units(), win.size, win.monitor)
            bh = to_pixels(float(size[1]), self._effective_units(), win.size, win.monitor)
            self._box_w = float(bw)
            self._box_h = float(bh)

        px, py = to_pixels(self._pos, self._effective_units(), win.size, win.monitor)
        assert isinstance(px, float) and isinstance(py, float)

        rgba = normalize_color(color, colorSpace, opacity) or (1.0, 1.0, 1.0, opacity)
        fill = normalize_color(fillColor, fillColorSpace, opacity) or (0.0, 0.0, 0.0, 0.0)
        lang = self._LANGUAGE_STYLE_MAP.get(languageStyle.lower(), LanguageStyle.LTR)

        self._handle: StimulusHandle = win._conn.stimuli.create_text(
            text=text,
            pos=StimulusVec2(px, py),
            box_width=self._box_w,
            box_height=self._box_h,
            letter_height=self._letter_height_px,
            font=font,
            anchor=anchor,
            color=StimulusColor(rgba[0], rgba[1], rgba[2], rgba[3]),
            fill_color=StimulusColor(fill[0], fill[1], fill[2], fill[3]),
            language_style=lang,
            name=name or "",
        )

        if autoDraw:
            self.autoDraw = True

    # ── Internal helpers ──────────────────────────────────────────────────────

    def _effective_units(self) -> str:
        return self._win._resolve_units(self._units)

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

    # ── text ─────────────────────────────────────────────────────────────────

    @property
    def text(self) -> str:
        return self._text

    @text.setter
    def text(self, value: str) -> None:
        self._text = str(value)
        self._win._dispatch(self._win._conn.stimuli.set_text, self._handle, self._text)

    def setText(self, value: str, log: bool | None = None) -> None:
        self.text = value

    # ── color ─────────────────────────────────────────────────────────────────

    @property
    def color(self) -> ColorInput:
        return self._color

    @color.setter
    def color(self, value: ColorInput) -> None:
        self._color = value
        self._resend_color()

    def setColor(self, value: ColorInput, colorSpace: str | None = None, log: bool | None = None) -> None:
        if colorSpace is not None:
            self._color_space = colorSpace
        self.color = value

    def _resend_color(self) -> None:
        rgba = normalize_color(self._color, self._color_space, self._opacity) or (1.0, 1.0, 1.0, self._opacity)
        self._win._dispatch(
            self._win._conn.stimuli.set_text_color,
            self._handle, StimulusColor(rgba[0], rgba[1], rgba[2], rgba[3]),
        )

    # ── opacity ───────────────────────────────────────────────────────────────

    @property
    def opacity(self) -> float:
        return self._opacity

    @opacity.setter
    def opacity(self, value: float) -> None:
        self._opacity = float(value)
        self._resend_color()

    def setOpacity(self, value: float, log: bool | None = None) -> None:
        self.opacity = value

    # ── pos ───────────────────────────────────────────────────────────────────

    @property
    def pos(self) -> tuple[float, float]:
        return self._pos

    @pos.setter
    def pos(self, value: Vec2) -> None:
        self._pos = (float(value[0]), float(value[1]))
        px, py = to_pixels(self._pos, self._effective_units(), self._win.size, self._win.monitor)
        assert isinstance(px, float) and isinstance(py, float)
        self._win._dispatch(self._win._conn.stimuli.set_position, self._handle, StimulusVec2(px, py))

    def setPos(self, value: Vec2, operation: str = "", log: bool | None = None) -> None:
        if operation == "+":
            value = (self._pos[0] + value[0], self._pos[1] + value[1])
        elif operation == "-":
            value = (self._pos[0] - value[0], self._pos[1] - value[1])
        self.pos = value

