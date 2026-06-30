// Virtual Trigger Line (VTL) client. Lines are addressed by {bank, bit} or by
// registered name; direction is a friendly string union.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import { VirtualTriggerLineDirection } from "./_proto/vstimd/v1/vtl_pb.js";
import { toVtlLineView, type VtlLineView } from "./snapshot.js";
import type { Send } from "./transport.js";

export type VtlDirection = "input" | "output";

/** Address a line by index or by registered name. */
export type VtlLine = { bank: number; bit: number } | string;

const DIR: Record<VtlDirection, VirtualTriggerLineDirection> = {
  input: VirtualTriggerLineDirection.INPUT,
  output: VirtualTriggerLineDirection.OUTPUT,
};

/** Build a proto VirtualTriggerLineHandle init from a {bank,bit} or name. Shared with animations. */
export function vtlLineHandle(line: VtlLine) {
  return typeof line === "string"
    ? { handle: { case: "name" as const, value: line } }
    : { handle: { case: "bankBit" as const, value: { bank: line.bank, bit: line.bit } } };
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

  /** Simulate a hardware input level. */
  async setInput(line: VtlLine, value: boolean): Promise<void> {
    await this.system({ case: "setInputVirtualTriggerLine", value: { handle: vtlLineHandle(line), value } });
  }

  async toggleInput(line: VtlLine): Promise<void> {
    await this.system({ case: "toggleInputVirtualTriggerLine", value: { handle: vtlLineHandle(line) } });
  }

  /** Drain accumulated rise/fall latches without changing the level. */
  async clearLatches(line: VtlLine): Promise<void> {
    await this.system({ case: "clearInputVirtualTriggerLineLatches", value: { handle: vtlLineHandle(line) } });
  }

  /** Manual output override (debugging — normally driven by the render loop). */
  async setOutput(line: VtlLine, value: boolean): Promise<void> {
    await this.system({ case: "setOutputVirtualTriggerLine", value: { handle: vtlLineHandle(line), value } });
  }

  async toggleOutput(line: VtlLine): Promise<void> {
    await this.system({ case: "toggleOutputVirtualTriggerLine", value: { handle: vtlLineHandle(line) } });
  }

  private system(body: MessageInitShape<typeof RequestSchema>["body"]): Promise<unknown> {
    return this.send(create(RequestSchema, { target: { case: "system", value: {} }, body }));
  }
}
