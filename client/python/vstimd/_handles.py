from typing import NewType

StimulusHandle = NewType("StimulusHandle", int)
"""Opaque integer handle returned by every ``create_*`` stimulus call.

Pass it to ``conn.stimuli`` methods to mutate or delete the stimulus, or to
animation ``create_*`` methods to attach an animation to it.
"""

AnimationHandle = NewType("AnimationHandle", int)
"""Opaque integer handle returned by every ``conn.animations.create_*`` call.

Pass it to :meth:`~vstimd.AnimationClient.arm`,
:meth:`~vstimd.AnimationClient.disarm`, and
:meth:`~vstimd.AnimationClient.delete`.
"""
