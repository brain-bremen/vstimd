// Virtual Trigger Line (VTL) client. Every line is addressed by a `VtlHandle`
// that always carries its direction (input vs. output): either {bank, bit} or a
// registered name. A single command family drives both directions.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import { VirtualTriggerLineDirection } from "./_proto/vstimd/v1/vtl_pb.js";
import { toVtlLineView, type VtlLineView } from "./snapshot.js";
import type { Send } from "./transport.js";

export type VtlDirection = "input" | "output";

/**
 * A fully-qualified VTL line address, carrying its direction. Address a line by
 * `{bank, bit}` or by registered `name` — never both. Construct via the
 * `VtlHandle` helpers.
 */
export type VtlHandle =
  | { direction: VtlDirection; bank: number; bit: number }
  | { direction: VtlDirection; name: string };

/** Constructors for {@link VtlHandle} (mirrors the type name). */
export const VtlHandle = {
  input: (bank: number, bit: number): VtlHandle => ({ direction: "input", bank, bit }),
  output: (bank: number, bit: number): VtlHandle => ({ direction: "output", bank, bit }),
  // Direction is explicit: a name may be registered for both directions.
  named: (name: string, direction: VtlDirection): VtlHandle => ({ direction, name }),
};

const DIR: Record<VtlDirection, VirtualTriggerLineDirection> = {
  input: VirtualTriggerLineDirection.INPUT,
  output: VirtualTriggerLineDirection.OUTPUT,
};

/** Build a proto VirtualTriggerLineHandle init from a {@link VtlHandle}. Shared with animations. */
export function vtlHandleProto(h: VtlHandle) {
  const direction = DIR[h.direction];
  return "name" in h
    ? { handle: { case: "name" as const, value: h.name }, direction }
    : { handle: { case: "bankBit" as const, value: { bank: h.bank, bit: h.bit } }, direction };
}

export class VtlClient {
  constructor(private readonly send: Send) {}

  /** List all registered VTL lines and their current state. */
  async list(): Promise<VtlLineView[]> {
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "listVirtualTriggerLines", value: {} },
      }),
    );
    const lines = resp.body.case === "virtualTriggerLineList" ? resp.body.value.lines : [];
    return lines.map(toVtlLineView);
  }

  /** Name (or rename) a line; empty name clears it. */
  async setName(bank: number, bit: number, direction: VtlDirection, name: string): Promise<void> {
    await this.system({
      case: "setVirtualTriggerLineName",
      value: { bank, bit, direction: DIR[direction], name },
    });
  }

  /** Drive a line high or low. An input handle simulates a hardware trigger;
   *  an output handle is a manual override (normally the render loop drives it). */
  async setLine(handle: VtlHandle, value: boolean): Promise<void> {
    await this.system({ case: "setVirtualTriggerLine", value: { handle: vtlHandleProto(handle), value } });
  }

  /** Toggle a line's level. */
  async toggleLine(handle: VtlHandle): Promise<void> {
    await this.system({ case: "toggleVirtualTriggerLine", value: { handle: vtlHandleProto(handle) } });
  }

  /** Drain an input line's rise/fall latches without changing its level. */
  async clearLatches(handle: VtlHandle): Promise<void> {
    await this.system({ case: "clearVirtualTriggerLineLatches", value: { handle: vtlHandleProto(handle) } });
  }

  /** Write all 64 lines of a bank at once (bitmask). Direction selects the bank. */
  async setBank(direction: VtlDirection, bank: number, value: bigint): Promise<void> {
    await this.system({ case: "setVirtualTriggerLineBank", value: { direction: DIR[direction], bank, value } });
  }

  private system(body: MessageInitShape<typeof RequestSchema>["body"]): Promise<unknown> {
    return this.send(create(RequestSchema, { target: { case: "system", value: {} }, body }));
  }
}
