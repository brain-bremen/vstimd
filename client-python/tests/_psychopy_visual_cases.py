"""Shared visual API test cases (psychopy-compatible).

Imported by test_visual.py (real server) and test_visual_null.py (null server).
Each function receives a `win` fixture — a wonderlamp.psychopy.visual.Window — so the
same cases run against both backends.
"""

import wonderlamp.psychopy.visual as visual


def test_create_rect(win: visual.Window) -> None:
    rect = visual.Rect(win, width=200, height=100, fillColor="red")
    rect.draw()
    win.flip()


def test_create_circle(win: visual.Window) -> None:
    circle = visual.Circle(win, radius=50, fillColor="blue")
    circle.draw()
    win.flip()
