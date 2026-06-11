# Types

```{eval-rst}
.. data:: vstimd.psychopy.visual.PsychoPyColor

   Any color value accepted by the PsychoPy-compatible layer.

   Accepted forms:

   * Named string: ``'red'``, ``'white'``, ``'black'``
   * Hex string: ``'#ff0000'``
   * PsychoPy ``rgb`` tuple (−1 … 1 per channel): ``(-1, 1, -1)``
   * Normalised float tuple (0 … 1 per channel): ``(1.0, 0.0, 0.0)``
   * ``rgb255`` tuple (0 … 255 per channel): ``(255, 0, 0)``
   * Scalar greyscale: ``0.5`` (float) or ``128`` (int)
   * ``None`` — transparent / no fill

.. data:: vstimd.psychopy.visual.PsychoPyVec2

   A 2-D position or size value accepted by the PsychoPy-compatible layer.

   Either a two-element tuple ``(x, y)`` or a two-element list ``[x, y]``.
   Units are interpreted according to the ``units`` parameter of the enclosing
   stimulus or window (``'pix'``, ``'norm'``, ``'height'``, ``'deg'``, ``'cm'``).
```
