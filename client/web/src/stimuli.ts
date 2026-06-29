// Stimulus creation and mutation client. Builds proto Requests internally and
// returns/accepts only public domain types (types.ts). Mirrors the Python
// client's vstimd.stimuli package.

import { create } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import type { Send } from "./transport.js";
import type { Color, StimulusHandle, Vec2 } from "./types.js";

const WHITE: Color = { r: 1, g: 1, b: 1, a: 1 };
const ORIGIN: Vec2 = { x: 0, y: 0 };

/** Shape-stimulus constructors (rect / circle / ellipse). */
export class ShapesClient {
  constructor(private readonly send: Send) {}

  async createRect(opts: {
    pos?: Vec2;
    width?: number;
    height?: number;
    color?: Color;
    name?: string;
  } = {}): Promise<StimulusHandle> {
    const { pos = ORIGIN, width = 100, height = 100, color = WHITE, name = "" } = opts;
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: {
          case: "createRect",
          value: { center: pos, width, height, fillColor: color, name },
        },
      }),
    );
    return resp.handle;
  }

  async createCircle(opts: {
    pos?: Vec2;
    radius?: number;
    color?: Color;
    name?: string;
  } = {}): Promise<StimulusHandle> {
    const { pos = ORIGIN, radius = 50, color = WHITE, name = "" } = opts;
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: {
          case: "createCircle",
          value: { center: pos, radius, fillColor: color, name },
        },
      }),
    );
    return resp.handle;
  }
}

/** Top-level stimulus client; generic mutations live here, shapes under `.shapes`. */
export class StimuliClient {
  readonly shapes: ShapesClient;

  constructor(private readonly send: Send) {
    this.shapes = new ShapesClient(send);
  }

  async setEnabled(handle: StimulusHandle, enabled: boolean): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "stimulus", value: handle },
        body: { case: "setEnabled", value: { enabled } },
      }),
    );
  }

  /** Move a stimulus. The hot path for receptive-field mapping. */
  async setPosition(handle: StimulusHandle, pos: Vec2): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "stimulus", value: handle },
        body: { case: "setPosition", value: { x: pos.x, y: pos.y } },
      }),
    );
  }

  async delete(handle: StimulusHandle): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "stimulus", value: handle },
        body: { case: "delete", value: {} },
      }),
    );
  }
}
