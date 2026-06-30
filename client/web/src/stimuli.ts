// Stimulus creation and mutation client. Builds proto Requests internally and
// returns/accepts only public domain types (types.ts). Mirrors the Python
// client's vstimd.stimuli package.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import { GratingClient } from "./grating.js";
import { TextClient } from "./text.js";
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

  async createEllipse(opts: {
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
          case: "createEllipse",
          value: { center: pos, width, height, fillColor: color, name },
        },
      }),
    );
    return resp.handle;
  }
}

/** Top-level stimulus client; generic mutations live here, shapes under `.shapes`. */
export class StimuliClient {
  readonly shapes: ShapesClient;
  readonly grating: GratingClient;
  readonly text: TextClient;

  constructor(private readonly send: Send) {
    this.shapes = new ShapesClient(send);
    this.grating = new GratingClient(send);
    this.text = new TextClient(send);
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

  /** Rotate a stimulus (degrees CCW). */
  async setOrientation(handle: StimulusHandle, angleDeg: number): Promise<void> {
    await this.stimulusCmd(handle, { case: "setOrientation", value: { angleDeg } });
  }

  async setRectSize(handle: StimulusHandle, width: number, height: number): Promise<void> {
    await this.stimulusCmd(handle, { case: "setRectSize", value: { width, height } });
  }

  async setCircleRadius(handle: StimulusHandle, radius: number): Promise<void> {
    await this.stimulusCmd(handle, { case: "setCircleRadius", value: { radius } });
  }

  async setEllipseSize(handle: StimulusHandle, width: number, height: number): Promise<void> {
    await this.stimulusCmd(handle, { case: "setEllipseSize", value: { width, height } });
  }

  async setFillColor(handle: StimulusHandle, color: Color): Promise<void> {
    await this.stimulusCmd(handle, { case: "setFillColor", value: { color } });
  }

  /** Set opacity in [0, 1]. */
  async setAlpha(handle: StimulusHandle, opacity: number): Promise<void> {
    await this.stimulusCmd(handle, { case: "setAlpha", value: { opacity } });
  }

  // Build + send a stimulus-targeted Request from a oneof body case.
  private stimulusCmd(
    handle: StimulusHandle,
    body: MessageInitShape<typeof RequestSchema>["body"],
  ): Promise<unknown> {
    return this.send(
      create(RequestSchema, { target: { case: "stimulus", value: handle }, body }),
    );
  }
}
