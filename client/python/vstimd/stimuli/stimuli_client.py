from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1.stimuli import query_pb2
from .stimuli_models import StimulusInfo
from ._base import _SendFn
from ._shapes import ShapesClient
from ._grating import GratingClient


class StimuliClient:
    """Top-level stimuli client; groups subclients by stimulus family."""

    def __init__(self, send: _SendFn) -> None:
        self.shapes = ShapesClient(send)
        self.grating = GratingClient(send)
        self._send = send

    def query(self, handle: StimulusHandle) -> StimulusInfo:
        """Return current server-side properties for the given stimulus handle."""
        req = service_pb2.Request(
            stimulus=handle,
            query_stimulus=query_pb2.QueryStimulusRequest(),
        )
        return StimulusInfo.from_proto(self._send(req).stimulus_info)
