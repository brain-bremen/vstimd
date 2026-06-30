// Public entry point — mirrors the Python client's Connection. Holds the two
// WebSocket channels and exposes namespaced sub-clients. Generated proto types
// never appear in this module's public surface.

import { ErrorCode, throwForCode } from "./errors.js";
import { CommandTransport, EventTransport, type Send } from "./transport.js";
import { toSceneSnapshot, type SceneSnapshot } from "./snapshot.js";
import { AnimationsClient } from "./animations.js";
import { ConfigClient } from "./config.js";
import { StimuliClient } from "./stimuli.js";
import { SystemClient } from "./system.js";
import { VtlClient } from "./vtl.js";

/** Callback for live scene snapshots from the /events channel. */
export type SnapshotListener = (snap: SceneSnapshot) => void;

/** A live subscription to the /events state stream. Call `close()` to stop. */
export interface EventSubscription {
  close(): void;
}

export class Connection {
  /** Create and mutate stimuli (`conn.stimuli.shapes.createRect(...)`). */
  readonly stimuli: StimuliClient;
  /** Scene-wide commands and server queries. */
  readonly system: SystemClient;
  /** Virtual trigger line control. */
  readonly vtl: VtlClient;
  /** Frame-accurate animations (`conn.animations.flash(...)`). */
  readonly animations: AnimationsClient;
  /** Named scene-config persistence (`conn.config.save/load/list`). */
  readonly config: ConfigClient;

  private constructor(
    private readonly cmd: CommandTransport,
    private readonly baseUrl: string,
  ) {
    // `send` mirrors the Python client's _send: dispatch, raise on non-OK, hand
    // the raw proto Response to the (internal) sub-clients for field mapping.
    const send: Send = async (req) => {
      const resp = await this.cmd.request(req);
      throwForCode(resp.code as number as ErrorCode, resp.error);
      return resp;
    };
    this.stimuli = new StimuliClient(send);
    this.system = new SystemClient(send);
    this.vtl = new VtlClient(send);
    this.animations = new AnimationsClient(send, () =>
      this.system.queryServerInfo().then((i) => i.frameRate),
    );
    this.config = new ConfigClient(send);
  }

  /**
   * Connect the command channel.
   *
   * @param baseUrl WebSocket origin, e.g. `ws://localhost:8080`.
   */
  static async connect(baseUrl = "ws://localhost:8080"): Promise<Connection> {
    const cmd = await CommandTransport.connect(baseUrl);
    return new Connection(cmd, baseUrl);
  }

  /** Subscribe to live scene snapshots (opens the /events channel). */
  async events(onSnapshot: SnapshotListener): Promise<EventSubscription> {
    const ev = await EventTransport.connect(this.baseUrl, (p) => onSnapshot(toSceneSnapshot(p)));
    return { close: () => ev.close() };
  }

  /** Resolve with the next single snapshot, then close the stream. */
  nextSnapshot(): Promise<SceneSnapshot> {
    return new Promise<SceneSnapshot>((resolve, reject) => {
      let sub: EventSubscription | undefined;
      let done = false;
      this.events((snap) => {
        if (done) return;
        done = true;
        resolve(snap);
        sub?.close();
      }).then((s) => {
        sub = s;
        if (done) s.close(); // snapshot already arrived before assignment
      }, reject);
    });
  }

  close(): void {
    this.cmd.close();
  }
}
