from __future__ import annotations

from ._colors import normalize_color
from ._types import ColorInput, Vec2
from ._units import to_pixels
from .window import Window


class Circle:
    """Circle (disc) stimulus.  API matches psychopy.visual.Circle."""

    def __init__(
        self,
        win: Window,
        radius: float = 0.5,
        edges: int | str = "circle",  # accepted for compat, server renders smooth disc
        units: str = "",
        pos: Vec2 = (0.0, 0.0),
        size: float = 1.0,
        ori: float = 0.0,
        fillColor: ColorInput = "white",
        lineColor: ColorInput = None,
        lineWidth: float = 1.5,
        colorSpace: str = "rgb",
        opacity: float = 1.0,
        contrast: float = 1.0,
        autoDraw: bool = False,
        name: str | None = None,
        autoLog: bool | None = None,
        # legacy
        depth: int = 0,
        interpolate: bool = True,
        draggable: bool = False,
        anchor: str | None = None,
        color: ColorInput = None,
        lineColorSpace: str | None = None,
        fillColorSpace: str | None = None,
        lineRGB: tuple[float, float, float] | None = None,
        fillRGB: tuple[float, float, float] | None = None,
    ) -> None:
        self._win = win
        self._units = units
        self._color_space = colorSpace
        self.name = name

        self._radius = float(radius)
        self._pos: tuple[float, float] = (float(pos[0]), float(pos[1]))
        self._ori = float(ori)
        self._opacity = float(opacity)
        self._fill_color: ColorInput = fillColor
        self._auto_draw = False

        px, py = self._to_px(self._pos)
        pr = self._scalar_px(self._radius)
        rgba = normalize_color(fillColor, colorSpace, opacity) or (0.0, 0.0, 0.0, 0.0)
        self._handle: int = win._conn.create_circle(
            x=px, y=py, radius=pr,
            r=rgba[0], g=rgba[1], b=rgba[2], a=rgba[3],
        )

        if autoDraw:
            self.autoDraw = True

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

    @property
    def autoDraw(self) -> bool:
        return self._auto_draw

    @autoDraw.setter
    def autoDraw(self, value: bool) -> None:
        self._auto_draw = bool(value)
        self._win._dispatch("set_enabled", self._handle, self._auto_draw)

    def setAutoDraw(self, value: bool, log: bool | None = None) -> None:
        self.autoDraw = value

    def draw(self) -> None:
        self._win._to_draw_once.append(self._handle)

    @property
    def pos(self) -> tuple[float, float]:
        return self._pos

    @pos.setter
    def pos(self, value: Vec2) -> None:
        self._pos = (float(value[0]), float(value[1]))
        px, py = self._to_px(self._pos)
        self._win._dispatch("move_circle", self._handle, px, py)

    def setPos(self, value: Vec2, operation: str = "", log: bool | None = None) -> None:
        if operation == "+":
            value = (self._pos[0] + value[0], self._pos[1] + value[1])
        elif operation == "-":
            value = (self._pos[0] - value[0], self._pos[1] - value[1])
        self.pos = value

    @property
    def radius(self) -> float:
        return self._radius

    @radius.setter
    def radius(self, value: float) -> None:
        self._radius = float(value)
        pr = self._scalar_px(self._radius)
        self._win._dispatch("resize_circle", self._handle, pr)

    def setRadius(self, value: float, log: bool | None = None) -> None:
        self.radius = value

    @property
    def ori(self) -> float:
        return self._ori

    @ori.setter
    def ori(self, value: float) -> None:
        self._ori = float(value)
        self._win._dispatch("set_angle", self._handle, self._ori)

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
    def fillColor(self) -> ColorInput:
        return self._fill_color

    @fillColor.setter
    def fillColor(self, value: ColorInput) -> None:
        self._fill_color = value
        self._resend_color()

    def setFillColor(self, value: ColorInput, colorSpace: str | None = None, log: bool | None = None) -> None:
        if colorSpace is not None:
            self._color_space = colorSpace
        self.fillColor = value

    def setColor(self, value: ColorInput, colorSpace: str | None = None, log: bool | None = None) -> None:
        self.setFillColor(value, colorSpace, log)

    def _resend_color(self) -> None:
        rgba = normalize_color(self._fill_color, self._color_space, self._opacity) or (0.0, 0.0, 0.0, 0.0)
        self._win._dispatch("set_circle_color", self._handle, *rgba)
