// Scene-wide commands and server queries. Mirrors vstimd.system.

import { create } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import type { QueryServerInfoResponse } from "./_proto/vstimd/v1/system_pb.js";
import type { Send } from "./transport.js";
import type { Color } from "./types.js";

/** Public server-info type — no proto leakage. */
export interface ServerInfo {
  width: number;
  height: number;
  frameRate: number;
  /** Semver string, e.g. "0.1.0". */
  version: string;
  background?: Color;
}

/** Map the proto server-info onto the public type. Shared with the snapshot stream. */
export function toServerInfo(info: QueryServerInfoResponse | undefined): ServerInfo {
  const v = info?.version;
  return {
    width: info?.width ?? 0,
    height: info?.height ?? 0,
    frameRate: info?.frameRate ?? 0,
    version: v ? `${v.major}.${v.minor}.${v.patch}` : "",
    background: info?.backgroundColor,
  };
}

export class SystemClient {
  constructor(private readonly send: Send) {}

  async queryServerInfo(): Promise<ServerInfo> {
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "queryServerInfo", value: {} },
      }),
    );
    return toServerInfo(resp.body.case === "serverInfo" ? resp.body.value : undefined);
  }

  /** Remove all stimuli from the scene. */
  async deleteAll(): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "deleteAll", value: {} },
      }),
    );
  }

  async setBackground(color: Color): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "setBackground", value: { color } },
      }),
    );
  }

  /** Enable or disable every stimulus at once (show/hide all). */
  async setAllEnabled(enabled: boolean): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "setAllEnabled", value: { enabled } },
      }),
    );
  }

  /**
   * Enter/exit deferred (frame-batched) mode. `active=true` begins; `active=false`
   * schedules an atomic flip on the next vsync; `cancel=true` discards pending
   * changes instead of applying them.
   */
  async setDeferredMode(active: boolean, cancel = false): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "setDeferredMode", value: { active, cancel } },
      }),
    );
  }
}
