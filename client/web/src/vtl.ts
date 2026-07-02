// Virtual Trigger Line (VTL) client. Every line is addressed by a `VtlHandle`
// that always carries its kind (input vs. output): either {bank, bit} or a
// registered name. A single command family drives both kinds.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import { VirtualTriggerLineKind } from "./_proto/vstimd/v1/vtl_pb.js";
import { toVtlLineView, type VtlLineView } from "./snapshot.js";
import type { Send } from "./transport.js";

export type VtlKind = "input" | "output";

/**
 * A fully-qualified VTL line address, carrying its kind. Address a line by
 * `{bank, bit}` or by registered `name` — never both. Construct via the
 * `VtlHandle` helpers.
 */
export type VtlHandle =
  | { kind: VtlKind; bank: number; bit: number }
  | { kind: VtlKind; name: string };

/** Constructors for {@link VtlHandle} (mirrors the type name). */
export const VtlHandle = {
  input: (bank: number, bit: number): VtlHandle => ({ kind: "input", bank, bit }),
  output: (bank: number, bit: number): VtlHandle => ({ kind: "output", bank, bit }),
  // Kind is explicit: a name may be registered for both kinds.
  named: (name: string, kind: VtlKind): VtlHandle => ({ kind, name }),
};

const DIR: Record<VtlKind, VirtualTriggerLineKind> = {
  input: VirtualTriggerLineKind.INPUT,
  output: VirtualTriggerLineKind.OUTPUT,
};

/** Build a proto VirtualTriggerLineHandle init from a {@link VtlHandle}. Shared with animations. */
export function vtlHandleProto(h: VtlHandle) {
  const kind = DIR[h.kind];
  return "name" in h
    ? { handle: { case: "name" as const, value: h.name }, kind }
    : { handle: { case: "bankBit" as const, value: { bank: h.bank, bit: h.bit } }, kind };
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
  async setName(bank: number, bit: number, kind: VtlKind, name: string): Promise<void> {
    await this.system({
      case: "setVirtualTriggerLineName",
      value: { bank, bit, kind: DIR[kind], name },
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

  /** Drain an input line's rise/fall latches without changing its level.
   *  Only valid for an input handle — outputs have no latches. */
  async clearLatches(handle: VtlHandle): Promise<void> {
    if (handle.kind === "output") {
      throw new Error("clearLatches is only valid for input lines (outputs have no latches)");
    }
    await this.system({ case: "clearVirtualTriggerLineLatches", value: { handle: vtlHandleProto(handle) } });
  }

  /** Write all 64 lines of a bank at once (bitmask). Kind selects the bank. */
  async setBank(kind: VtlKind, bank: number, value: bigint): Promise<void> {
    await this.system({ case: "setVirtualTriggerLineBank", value: { kind: DIR[kind], bank, value } });
  }

  private system(body: MessageInitShape<typeof RequestSchema>["body"]): Promise<unknown> {
    return this.send(create(RequestSchema, { target: { case: "system", value: {} }, body }));
  }
}
