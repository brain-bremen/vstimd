# Connection

```{eval-rst}
.. autoclass:: vstimd.Connection
   :members:
   :undoc-members:
```

## ServerResponse

Returned by every mutation command.  Carries server-side timing metadata and
the error code (always `OK` — exceptions are raised on any other code).

```{eval-rst}
.. autoclass:: vstimd.ServerResponse
   :no-members:
   :no-undoc-members:

.. autoclass:: vstimd.ErrorCode
   :members:
```
