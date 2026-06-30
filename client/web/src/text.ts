// Text stimulus client.

import { create } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import type { Send } from "./transport.js";
import type { Color, StimulusHandle, Vec2 } from "./types.js";

export class TextClient {
  constructor(private readonly send: Send) {}

  async create(opts: {
    text: string;
    pos?: Vec2;
    font?: string;
    letterHeight?: number;
    color?: Color;
    name?: string;
  }): Promise<StimulusHandle> {
    const { text, pos = { x: 0, y: 0 }, font = "", letterHeight = 32, color, name = "" } = opts;
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: {
          case: "createText",
          value: { text, font, letterHeight, pos, color, name },
        },
      }),
    );
    return resp.handle;
  }

  async setText(handle: StimulusHandle, text: string): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "stimulus", value: handle },
        body: { case: "setText", value: { text } },
      }),
    );
  }

  async setColor(handle: StimulusHandle, color: Color): Promise<void> {
    await this.send(
      create(RequestSchema, {
        target: { case: "stimulus", value: handle },
        body: { case: "setTextColor", value: { color } },
      }),
    );
  }
}
