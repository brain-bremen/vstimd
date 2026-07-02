from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Optional

from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import vtl_pb2
from vstimd.response import ServerResponse
from .vtl_models import VtlDirection, VtlLineInfo


_SendFn = Callable[[service_pb2.Request], service_pb2.Response]

_DIRECTION_TO_PROTO: dict[VtlDirection, vtl_pb2.VirtualTriggerLineDirection] = {
    VtlDirection.INPUT:  vtl_pb2.VIRTUAL_TRIGGER_LINE_DIRECTION_INPUT,
    VtlDirection.OUTPUT: vtl_pb2.VIRTUAL_TRIGGER_LINE_DIRECTION_OUTPUT,
}


@dataclass(frozen=True)
class VtlHandle:
    """A fully-qualified VTL line address, carrying its direction.

    A VTL address is only unambiguous once its direction is known — input and
    output banks are independent signals sharing the same (bank, bit) space.
    Use this wherever the direction is part of the choice (e.g. an animation
    ``start_trigger`` / ``cancel_trigger`` that may fire off an input *or* an
    output edge). Construct with the classmethods::

        VtlHandle.input(0, 10)                    # input bank, bit 10
        VtlHandle.output(0, 20)                   # output bank, bit 20
        VtlHandle.named("frame_sync", VtlDirection.OUTPUT)  # a registered line
    """

    direction: VtlDirection
    bank: Optional[int] = None
    bit: Optional[int] = None
    name: Optional[str] = None

    def __post_init__(self) -> None:
        has_addr = self.bank is not None and self.bit is not None
        if self.name is not None:
            if has_addr:
                raise ValueError(
                    "VtlHandle: give either a name or a (bank, bit) pair, not both"
                )
        elif not has_addr:
            raise ValueError("VtlHandle needs a name or a (bank, bit) pair")

    @classmethod
    def input(cls, bank: int, bit: int) -> "VtlHandle":
        return cls(VtlDirection.INPUT, bank=bank, bit=bit)

    @classmethod
    def output(cls, bank: int, bit: int) -> "VtlHandle":
        return cls(VtlDirection.OUTPUT, bank=bank, bit=bit)

    @classmethod
    def named(cls, name: str, direction: VtlDirection) -> "VtlHandle":
        # Direction is explicit: the server matches the registered entry with
        # this name *and* direction (a name may exist for both directions).
        return cls(direction, name=name)

    def _to_proto(self) -> vtl_pb2.VirtualTriggerLineHandle:
        dir_proto = _DIRECTION_TO_PROTO[self.direction]
        if self.name is not None:
            return vtl_pb2.VirtualTriggerLineHandle(name=self.name, direction=dir_proto)
        return vtl_pb2.VirtualTriggerLineHandle(
            bank_bit=vtl_pb2.VirtualTriggerLineBankBit(bank=self.bank, bit=self.bit),
            direction=dir_proto,
        )


def _sys() -> service_pb2.SystemTarget:
    return service_pb2.SystemTarget()


class VtlClient:
    """Virtual Trigger Line (VTL) commands.

    Accessed as ``conn.vtl`` on a :class:`~vstimd.Connection` instance.

    Every line is addressed by a :class:`VtlHandle`, which carries its direction
    (input vs. output). ``set_line`` / ``toggle_line`` / ``clear_latches`` all
    take a handle; there is a single server-side command family behind them.

    **Input lines** represent signals arriving into vstimd.  The canonical
    writer is daqd (hardware DAQ) via shared memory; ``set_line`` on an INPUT
    handle simulates that path.  The render loop polls input lines once per frame
    at the start of each frame, detects rising/falling edges, and feeds them to
    the animation system.

    **Output lines** represent signals driven by vstimd.  The render loop
    writes output lines once per frame at the end of each frame (after vsync);
    daqd is woken by the output semaphore strobe and reads ``output_state`` to
    pulse hardware DAQ lines (frame-sync, stimulus-onset markers). Setting an
    OUTPUT handle here is a manual override for testing.

    Example::

        with Connection() as conn:
            conn.vtl.set_line_name(0, 0, VtlDirection.OUTPUT, "frame_sync")
            conn.vtl.set_line(VtlHandle.named("frame_sync", VtlDirection.OUTPUT), True)
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
    ) -> ServerResponse:
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
        return ServerResponse._from_proto(self._send(req))

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

    # ── Line control ────────────────────────────────────────────────────────────

    def set_line(self, handle: VtlHandle, value: bool) -> ServerResponse:
        """Drive a line high or low. The handle's direction selects the bank: an
        INPUT handle simulates a hardware trigger input; an OUTPUT handle is a
        manual override (normally the render loop drives outputs)."""
        req = service_pb2.Request(
            system=_sys(),
            set_virtual_trigger_line=vtl_pb2.SetVirtualTriggerLineRequest(
                handle=handle._to_proto(),
                value=value,
            ),
        )
        return ServerResponse._from_proto(self._send(req))

    def toggle_line(self, handle: VtlHandle) -> bool:
        """Toggle a line and return the new state."""
        req = service_pb2.Request(
            system=_sys(),
            toggle_virtual_trigger_line=vtl_pb2.ToggleVirtualTriggerLineRequest(
                handle=handle._to_proto(),
            ),
        )
        resp = self._send(req)
        return resp.virtual_trigger_line_state.high

    def clear_latches(self, handle: VtlHandle) -> ServerResponse:
        """Drain an input line's accumulated rise/fall latches without changing
        its level. Only valid for an INPUT handle (outputs have no latches)."""
        req = service_pb2.Request(
            system=_sys(),
            clear_virtual_trigger_line_latches=vtl_pb2.ClearVirtualTriggerLineLatchesRequest(
                handle=handle._to_proto(),
            ),
        )
        return ServerResponse._from_proto(self._send(req))

    def set_bank(self, direction: VtlDirection, bank: int, value: int) -> ServerResponse:
        """Write all 64 lines of a bank at once (bitmask). Direction selects the
        input or output bank."""
        req = service_pb2.Request(
            system=_sys(),
            set_virtual_trigger_line_bank=vtl_pb2.SetVirtualTriggerLineBankRequest(
                direction=_DIRECTION_TO_PROTO[direction],
                bank=bank,
                value=value,
            ),
        )
        return ServerResponse._from_proto(self._send(req))
