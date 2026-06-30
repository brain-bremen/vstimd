// Grating stimulus client. Exposes friendly string unions for waveform/mask and
// maps them to the proto enums internally.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import { MaskType, WaveformType } from "./_proto/vstimd/v1/stimuli/grating_pb.js";
import type { Send } from "./transport.js";
import type { Color, StimulusHandle, Vec2 } from "./types.js";

export type Waveform = "sin" | "sqr" | "saw" | "tri";
export type GratingMask = "none" | "circle" | "gauss" | "hann" | "raisedCos";

const WAVEFORM: Record<Waveform, WaveformType> = {
  sin: WaveformType.SIN,
  sqr: WaveformType.SQR,
  saw: WaveformType.SAW,
  tri: WaveformType.TRI,
};
const MASK: Record<GratingMask, MaskType> = {
  none: MaskType.NONE,
  circle: MaskType.CIRCLE,
  gauss: MaskType.GAUSS,
  hann: MaskType.HANN,
  raisedCos: MaskType.RAISED_COS,
};

export class GratingClient {
  constructor(private readonly send: Send) {}

  async create(opts: {
    pos?: Vec2;
    width?: number;
    height?: number;
    sf?: number;
    phase?: number;
    angle?: number;
    contrast?: number;
    foreColor?: Color;
    backColor?: Color;
    opacity?: number;
    waveform?: Waveform;
    mask?: GratingMask;
    driftSpeed?: number;
    name?: string;
  } = {}): Promise<StimulusHandle> {
    const {
      pos = { x: 0, y: 0 }, width = 200, height = 200, sf = 0.05, phase = 0,
      angle = 0, contrast = 1, foreColor, backColor, opacity = 1,
      waveform = "sin", mask = "none", driftSpeed = 0, name = "",
    } = opts;
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: {
          case: "createGrating",
          value: {
            center: pos, width, height, sf, phase, angle, contrast,
            foreColor, backColor, opacity,
            waveform: WAVEFORM[waveform], mask: MASK[mask], driftSpeed, name,
          },
        },
      }),
    );
    return resp.handle;
  }

  setSf(h: StimulusHandle, sf: number) { return this.cmd(h, { case: "setGratingSf", value: { sf } }); }
  setContrast(h: StimulusHandle, contrast: number) { return this.cmd(h, { case: "setGratingContrast", value: { contrast } }); }
  setPhase(h: StimulusHandle, phase: number) { return this.cmd(h, { case: "setGratingPhase", value: { phase } }); }
  setDriftSpeed(h: StimulusHandle, speed: number) { return this.cmd(h, { case: "setGratingDriftSpeed", value: { speed } }); }
  setOpacity(h: StimulusHandle, opacity: number) { return this.cmd(h, { case: "setGratingOpacity", value: { opacity } }); }
  setWaveform(h: StimulusHandle, w: Waveform) { return this.cmd(h, { case: "setGratingWaveform", value: { waveform: WAVEFORM[w] } }); }
  setMask(h: StimulusHandle, m: GratingMask) { return this.cmd(h, { case: "setGratingMask", value: { mask: MASK[m] } }); }
  setForeColor(h: StimulusHandle, foreColor: Color) { return this.cmd(h, { case: "setGratingForeColor", value: { foreColor } }); }
  setBackColor(h: StimulusHandle, backColor: Color) { return this.cmd(h, { case: "setGratingBackColor", value: { backColor } }); }

  private cmd(handle: StimulusHandle, body: MessageInitShape<typeof RequestSchema>["body"]): Promise<unknown> {
    return this.send(create(RequestSchema, { target: { case: "stimulus", value: handle }, body }));
  }
}
