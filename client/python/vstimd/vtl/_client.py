from __future__ import annotations

from typing import Callable, Union

from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import vtl_pb2
from ._models import VtlDirection, VtlLineInfo


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]

# A VTL line can be addressed by (bank, bit) or by registered name.
VtlHandle = Union[tuple[int, int], str]

_DIRECTION_TO_PROTO: dict[VtlDirection, vtl_pb2.VirtualTriggerLineDirection] = {
    VtlDirection.INPUT:  vtl_pb2.VIRTUAL_TRIGGER_LINE_DIRECTION_INPUT,
    VtlDirection.OUTPUT: vtl_pb2.VIRTUAL_TRIGGER_LINE_DIRECTION_OUTPUT,
}


def _make_handle(handle: VtlHandle) -> vtl_pb2.VirtualTriggerLineHandle:
    if isinstance(handle, str):
        return vtl_pb2.VirtualTriggerLineHandle(name=handle)
    bank, bit = handle
    return vtl_pb2.VirtualTriggerLineHandle(
        bank_bit=vtl_pb2.VirtualTriggerLineBankBit(bank=bank, bit=bit)
    )


def _sys() -> service_pb2.SystemTarget:
    return service_pb2.SystemTarget()


class VtlClient:
    """Virtual Trigger Line commands.

    **Input lines** represent signals arriving into vstimd.  The canonical
    writer is nidaqd (hardware DAQ) via shared memory; ZMQ ``SetInput*``
    commands simulate that path.  The render loop is designed to poll input
    lines once per frame at the *start* of each frame, detect rising/falling
    edges, and feed them to the animation system.

    **Output lines** represent signals driven by vstimd.  The render loop is
    designed to write output lines once per frame at the *end* of each frame
    (after vsync).  nidaqd reads ``output_state`` to pulse hardware DAQ lines
    (frame-sync, stimulus-onset markers).  ZMQ ``SetOutput*`` commands are a
    manual override for testing.

    Note: render-loop integration (frame-gated poll and output write) is not
    yet implemented.  Both directions are currently only accessible via ZMQ.
    """

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    # ── Naming ────────────────────────────────────────────────────────────────

    def set_line_name(
        self,
        bank: int,
        bit: int,
        direction: VtlDirection,
        name: str,
    ) -> None:
        """Register a name for a VTL line. Pass ``name=""`` to clear."""
        req = service_pb2.Request(
            system=_sys(),
            set_virtual_trigger_line_name=vtl_pb2.SetVirtualTriggerLineNameRequest(
                bank=bank,
                bit=bit,
                direction=_DIRECTION_TO_PROTO[direction],
                name=name,
            ),
        )
        self._send(req)

    def list_lines(self) -> list[VtlLineInfo]:
        """Return all named VTL lines and their current state."""
        req = service_pb2.Request(
            system=_sys(),
            list_virtual_trigger_lines=vtl_pb2.ListVirtualTriggerLinesRequest(),
        )
        resp = self._send(req)
        return [
            VtlLineInfo(
                name=line.name,
                bank=line.bank,
                bit=line.bit,
                direction=VtlDirection(line.direction),
                high=line.high,
            )
            for line in resp.virtual_trigger_line_list.lines
        ]

    # ── Input line control ────────────────────────────────────────────────────

    def set_input_line(self, handle: VtlHandle, value: bool) -> None:
        """Drive an input line high or low (simulates a hardware trigger input)."""
        req = service_pb2.Request(
            system=_sys(),
            set_input_virtual_trigger_line=vtl_pb2.SetInputVirtualTriggerLineRequest(
                handle=_make_handle(handle),
                value=value,
            ),
        )
        self._send(req)

    def toggle_input_line(self, handle: VtlHandle) -> bool:
        """Toggle an input line and return the new state."""
        req = service_pb2.Request(
            system=_sys(),
            toggle_input_virtual_trigger_line=vtl_pb2.ToggleInputVirtualTriggerLineRequest(
                handle=_make_handle(handle),
            ),
        )
        resp = self._send(req)
        return resp.virtual_trigger_line_state.high

    def clear_input_latches(self, handle: VtlHandle) -> None:
        """Drain accumulated rise/fall latches without changing the line level."""
        req = service_pb2.Request(
            system=_sys(),
            clear_input_virtual_trigger_line_latches=vtl_pb2.ClearInputVirtualTriggerLineLatchesRequest(
                handle=_make_handle(handle),
            ),
        )
        self._send(req)

    def set_input_bank(self, bank: int, value: int) -> None:
        """Write all 64 input lines of a bank at once (bitmask)."""
        req = service_pb2.Request(
            system=_sys(),
            set_input_virtual_trigger_line_bank=vtl_pb2.SetInputVirtualTriggerLineBankRequest(
                bank=bank,
                value=value,
            ),
        )
        self._send(req)

    # ── Output line control ───────────────────────────────────────────────────

    def set_output_line(self, handle: VtlHandle, value: bool) -> None:
        """Drive an output line high or low."""
        req = service_pb2.Request(
            system=_sys(),
            set_output_virtual_trigger_line=vtl_pb2.SetOutputVirtualTriggerLineRequest(
                handle=_make_handle(handle),
                value=value,
            ),
        )
        self._send(req)

    def toggle_output_line(self, handle: VtlHandle) -> bool:
        """Toggle an output line and return the new state."""
        req = service_pb2.Request(
            system=_sys(),
            toggle_output_virtual_trigger_line=vtl_pb2.ToggleOutputVirtualTriggerLineRequest(
                handle=_make_handle(handle),
            ),
        )
        resp = self._send(req)
        return resp.virtual_trigger_line_state.high

    def set_output_bank(self, bank: int, value: int) -> None:
        """Write all 64 output lines of a bank at once (bitmask)."""
        req = service_pb2.Request(
            system=_sys(),
            set_output_virtual_trigger_line_bank=vtl_pb2.SetOutputVirtualTriggerLineBankRequest(
                bank=bank,
                value=value,
            ),
        )
        self._send(req)
