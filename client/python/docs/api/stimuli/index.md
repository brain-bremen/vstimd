# Stimuli

```{toctree}
:maxdepth: 1
:hidden:

shapes
grating
text
```

## StimuliClient

```{eval-rst}
.. autoclass:: vstimd.stimuli.StimuliClient
   :members:
   :undoc-members:
```

## Shared types

```{eval-rst}
.. autoclass:: vstimd.stimuli.Color
   :members:

.. autoclass:: vstimd.stimuli.Vec2
   :members:

.. data:: vstimd.StimulusHandle

   Opaque integer handle returned by every ``create_*`` stimulus call.
   Pass it to ``conn.stimuli`` methods to mutate or delete the stimulus, or to
   animation ``create_*`` methods to attach an animation to it.

.. autoclass:: vstimd.stimuli.StimulusType
   :members:

.. autoclass:: vstimd.stimuli.StimulusInfo
   :members:
```
