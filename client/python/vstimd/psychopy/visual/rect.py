from __future__ import annotations

from ..._handles import StimulusHandle
from ...stimuli.stimuli_models import Color as StimulusColor, Vec2 as StimulusVec2
from ._colors import to_color
from ._types import PsychoPyColor, PsychoPyVec2
from ._units import to_pixels
from .window import Window


class Rect:
    """Rectangle stimulus.  API matches psychopy.visual.Rect."""

    def __init__(
        self,
        win: Window,
        width: float = 0.5,
        height: float = 0.5,
        units: str = "",
        pos: PsychoPyVec2 = (0.0, 0.0),
        size: PsychoPyVec2 | float | None = None,
        ori: float = 0.0,
        fillColor: PsychoPyColor = "white",
        lineColor: PsychoPyColor = None,
        lineWidth: float = 1.5,
        colorSpace: str = "rgb",
        opacity: float = 1.0,
        contrast: float = 1.0,
        autoDraw: bool = False,
        name: str | None = None,
        autoLog: bool | None = None,
        # legacy psychopy params — accepted, ignored
        depth: int = 0,
        interpolate: bool = True,
        draggable: bool = False,
        anchor: str | None = None,
        color: PsychoPyColor = None,
        lineColorSpace: str | None = None,
        fillColorSpace: str | None = None,
        lineRGB: tuple[float, float, float] | None = None,
        fillRGB: tuple[float, float, float] | None = None,
    ) -> None:
        self._win = win
        self._units = units
        self._color_space = colorSpace
        self.name = name

        if size is not None:
            if isinstance(size, (int, float)):
                width = height = float(size)
            else:
                width, height = float(size[0]), float(size[1])

        self._width = float(width)
        self._height = float(height)
        self._pos: tuple[float, float] = (float(pos[0]), float(pos[1]))
        self._ori = float(ori)
        self._opacity = float(opacity)
        self._fill_color: PsychoPyColor = fillColor
        self._auto_draw = False

        px, py = self._to_px(self._pos)
        pw = self._scalar_px(self._width)
        ph = self._scalar_px(self._height)
        self._handle: StimulusHandle = win._conn.stimuli.shapes.create_rect(
            pos=StimulusVec2(px, py), width=pw, height=ph,
            color=to_color(fillColor, colorSpace, opacity) or StimulusColor(0.0, 0.0, 0.0, 0.0),
        )

        if autoDraw:
            self.autoDraw = True

    def _effective_units(self) -> str:
        return self._win._resolve_units(self._units)

    def _to_px(self, pos: PsychoPyVec2) -> tuple[float, float]:
        result = to_pixels(pos, self._effective_units(), self._win.size, self._win.monitor)
        assert isinstance(result, tuple)
        return result

    def _scalar_px(self, val: float) -> float:
        result = to_pixels(val, self._effective_units(), self._win.size, self._win.monitor)
        assert isinstance(result, float)
        return result

    @property
    def autoDraw(self) -> bool:
        return self._auto_draw

    @autoDraw.setter
    def autoDraw(self, value: bool) -> None:
        self._auto_draw = bool(value)
        self._win._dispatch(self._win._conn.stimuli.shapes.set_enabled, self._handle, self._auto_draw)

    def setAutoDraw(self, value: bool, log: bool | None = None) -> None:
        self.autoDraw = value

    def draw(self) -> None:
        self._win._to_draw_once.append(self._handle)

    @property
    def pos(self) -> tuple[float, float]:
        return self._pos

    @pos.setter
    def pos(self, value: PsychoPyVec2) -> None:
        self._pos = (float(value[0]), float(value[1]))
        px, py = self._to_px(self._pos)
        self._win._dispatch(self._win._conn.stimuli.shapes.set_position, self._handle, StimulusVec2(px, py))

    def setPos(self, value: PsychoPyVec2, operation: str = "", log: bool | None = None) -> None:
        if operation == "+":
            value = (self._pos[0] + value[0], self._pos[1] + value[1])
        elif operation == "-":
            value = (self._pos[0] - value[0], self._pos[1] - value[1])
        self.pos = value

    @property
    def size(self) -> tuple[float, float]:
        return (self._width, self._height)

    @size.setter
    def size(self, value: PsychoPyVec2 | float) -> None:
        if isinstance(value, (int, float)):
            self._width = self._height = float(value)
        else:
            self._width, self._height = float(value[0]), float(value[1])
        pw = self._scalar_px(self._width)
        ph = self._scalar_px(self._height)
        self._win._dispatch(self._win._conn.stimuli.shapes.set_rect_size, self._handle, pw, ph)

    def setSize(self, value: PsychoPyVec2 | float, operation: str = "", log: bool | None = None) -> None:
        self.size = value

    @property
    def width(self) -> float:
        return self._width

    @width.setter
    def width(self, value: float) -> None:
        self.size = (float(value), self._height)

    @property
    def height(self) -> float:
        return self._height

    @height.setter
    def height(self, value: float) -> None:
        self.size = (self._width, float(value))

    @property
    def ori(self) -> float:
        return self._ori

    @ori.setter
    def ori(self, value: float) -> None:
        self._ori = float(value)
        self._win._dispatch(self._win._conn.stimuli.shapes.set_orientation, self._handle, self._ori)

    def setOri(self, value: float, operation: str = "", log: bool | None = None) -> None:
        self.ori = value

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
    def fillColor(self) -> PsychoPyColor:
        return self._fill_color

    @fillColor.setter
    def fillColor(self, value: PsychoPyColor) -> None:
        self._fill_color = value
        self._resend_color()

    def setFillColor(self, value: PsychoPyColor, colorSpace: str | None = None, log: bool | None = None) -> None:
        if colorSpace is not None:
            self._color_space = colorSpace
        self.fillColor = value

    def setColor(self, value: PsychoPyColor, colorSpace: str | None = None, log: bool | None = None) -> None:
        self.setFillColor(value, colorSpace, log)

    def _resend_color(self) -> None:
        self._win._dispatch(
            self._win._conn.stimuli.shapes.set_fill_color,
            self._handle, to_color(self._fill_color, self._color_space, self._opacity) or StimulusColor(0.0, 0.0, 0.0, 0.0),
        )
