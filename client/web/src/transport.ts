// Internal WebSocket transport. The only module besides the sub-clients that
// touches generated protobuf-es code. Not part of the public API.
//
//   /ws      command channel: send Request, await Response (serialised REQ/REP)
//   /events  state channel:   receive SceneSnapshot frames (push only)

import { fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  RequestSchema,
  ResponseSchema,
  type Request,
  type Response,
} from "./_proto/vstimd/v1/service_pb.js";
import {
  SceneSnapshotSchema,
  type SceneSnapshot as ProtoSnapshot,
} from "./_proto/vstimd/v1/snapshot_pb.js";

function openSocket(url: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    ws.onopen = () => resolve(ws);
    ws.onerror = (e) => reject(new Error(`WebSocket connect failed: ${url} (${String(e)})`));
  });
}

/** Command channel: serialises requests so responses match the REQ/REP server. */
export class CommandTransport {
  private chain: Promise<unknown> = Promise.resolve();

  private constructor(private readonly ws: WebSocket) {}

  static async connect(baseUrl: string): Promise<CommandTransport> {
    return new CommandTransport(await openSocket(`${baseUrl}/ws`));
  }

  request(req: Request): Promise<Response> {
    // Serialise: one in-flight request at a time, matching the server's loop.
    const result = this.chain.then(() => this.sendOne(req));
    this.chain = result.catch(() => undefined);
    return result;
  }

  private sendOne(req: Request): Promise<Response> {
    return new Promise((resolve, reject) => {
      const onMessage = (ev: MessageEvent) => {
        cleanup();
        try {
          resolve(fromBinary(ResponseSchema, new Uint8Array(ev.data as ArrayBuffer)));
        } catch (err) {
          reject(err);
        }
      };
      const onError = () => {
        cleanup();
        reject(new Error("command channel socket error"));
      };
      const cleanup = () => {
        this.ws.removeEventListener("message", onMessage);
        this.ws.removeEventListener("error", onError);
      };
      this.ws.addEventListener("message", onMessage);
      this.ws.addEventListener("error", onError);
      this.ws.send(toBinary(RequestSchema, req));
    });
  }

  close(): void {
    this.ws.close();
  }
}

/** State channel: decodes pushed SceneSnapshot frames. */
export class EventTransport {
  private constructor(private readonly ws: WebSocket) {}

  static async connect(
    baseUrl: string,
    onSnapshot: (snap: ProtoSnapshot) => void,
  ): Promise<EventTransport> {
    const ws = await openSocket(`${baseUrl}/events`);
    ws.addEventListener("message", (ev: MessageEvent) => {
      onSnapshot(fromBinary(SceneSnapshotSchema, new Uint8Array(ev.data as ArrayBuffer)));
    });
    return new EventTransport(ws);
  }

  close(): void {
    this.ws.close();
  }
}

/** Function injected into sub-clients, mirroring the Python client's `_send`. */
export type Send = (req: Request) => Promise<Response>;
