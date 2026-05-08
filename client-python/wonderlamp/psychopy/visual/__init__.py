"""PsychoPy-compatible visual stimulus layer for wonderlamp_server.

    from wonderlamp import visual

    win = visual.Window(size=(1920, 1080), address='tcp://192.168.1.10:5555')
    circ = visual.Circle(win, radius=50, fillColor='red')
    rect = visual.Rect(win, width=200, height=100, fillColor=(-1, 1, -1))
    circ.draw()
    win.flip()
"""

from .window import Window
from .rect import Rect
from .circle import Circle

__all__ = ["Window", "Rect", "Circle"]
