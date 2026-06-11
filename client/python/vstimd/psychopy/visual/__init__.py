"""PsychoPy-compatible visual stimulus layer for vstimd.

    from vstimd import visual

    win = visual.Window(size=(1920, 1080), address='tcp://192.168.1.10:5555')
    circ = visual.Circle(win, radius=50, fillColor='red')
    rect = visual.Rect(win, width=200, height=100, fillColor=(-1, 1, -1))
    circ.draw()
    win.flip()
"""

from .window import Window
from .rect import Rect
from .circle import Circle
from .grating import GratingStim, GratingMask, GratingTexture
from .text import TextBox2
from ._types import PsychoPyColor, PsychoPyVec2

__all__ = ["Window", "Rect", "Circle", "GratingStim", "GratingMask", "GratingTexture", "TextBox2", "PsychoPyColor", "PsychoPyVec2"]
