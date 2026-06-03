# PsychoPy Layer

`vstimd.psychopy` is a drop-in replacement for `psychopy.visual` for experiments that already
use PsychoPy's API. It wraps the vstimd connection and maps PsychoPy conventions (colour
space, units) to vstimd commands.

## Usage

```python
from vstimd.psychopy import Window, Rect, Circle, GratingStim

win = Window(size=(1920, 1080), address="tcp://localhost:5555")

rect = Rect(win, width=0.5, height=0.25, fillColor="red", pos=(0, 0))
circ = Circle(win, radius=0.1, fillColor=(-1, 1, -1))   # PsychoPy RGB [-1, 1]
grat = GratingStim(win, sf=4, size=0.5, ori=45)

rect.draw()
circ.draw()
grat.draw()
win.flip()
```

## Coordinate units

The PsychoPy layer uses `units="norm"` by default (normalised: ±1 from centre).
Pass `units="pix"` to use raw pixel coordinates matching the native vstimd API.

## API reference

::: vstimd.psychopy.visual.Window

::: vstimd.psychopy.visual.Rect

::: vstimd.psychopy.visual.Circle

::: vstimd.psychopy.visual.GratingStim

::: vstimd.psychopy.visual.GratingMask

::: vstimd.psychopy.visual.GratingTexture
