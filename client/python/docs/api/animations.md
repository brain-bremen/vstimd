# Animations

```{eval-rst}
.. data:: vstimd.AnimationHandle

   Opaque integer handle returned by every ``conn.animations.create_*`` call.
   Pass it to :meth:`~vstimd.AnimationClient.arm`,
   :meth:`~vstimd.AnimationClient.disarm`, and
   :meth:`~vstimd.AnimationClient.delete`.

.. autoclass:: vstimd.AnimationClient
   :members:
   :undoc-members:

.. autoclass:: vstimd.AnimationDetails
   :members:

.. autoclass:: vstimd.AnimationInfo
   :members:

.. autoclass:: vstimd.AnimationState
   :members:

.. autoclass:: vstimd.FinalAction
   :members:

.. autoclass:: vstimd.StartAction
   :members:

.. autoclass:: vstimd.VtlEdge
   :members:
```
