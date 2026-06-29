// Public domain view of a scene snapshot, mapped from the proto SceneSnapshot
// pushed on the /events channel. User code (and the React read model) consumes
// these types, never the generated proto.
//
// Each stimulus carries both its stable UUID `id` and its u32 server `handle`
// (the map key used to address mutations like SetPosition during RF mapping).

import type { SceneSnapshot as ProtoSnapshot } from "./_proto/vstimd/v1/snapshot_pb.js";
import { StimulusType } from "./_proto/vstimd/v1/stimuli/stimulus_type_pb.js";
import { toServerInfo, type ServerInfo } from "./system.js";
import type { Color, StimulusHandle, StimulusKind, Vec2 } from "./types.js";

export interface StimulusView {
  /** Stable UUID assigned at creation. */
  id: string;
  /** Human-readable label ("" if unset). */
  name: string;
  /** Server handle (map key) — addresses mutations like setPosition. */
  handle: StimulusHandle;
  kind: StimulusKind;
  pos: Vec2;
  /** Orientation in degrees CCW. */
  orientation: number;
  opacity: number;
  fillColor?: Color;
  enabled: boolean;
  drawOrder: number;
}

export interface SceneSnapshot {
  serverInfo?: ServerInfo;
  stimuli: StimulusView[];
  frameCount: bigint;
  serverTimeNs: bigint;
}

function kindOf(t: StimulusType): StimulusKind {
  switch (t) {
    case StimulusType.RECT: return "rect";
    case StimulusType.CIRCLE: return "circle";
    case StimulusType.ELLIPSE: return "ellipse";
    case StimulusType.GRATING: return "grating";
    case StimulusType.TEXT: return "text";
    case StimulusType.POLYGON: return "polygon";
    default: return "unknown";
  }
}

export function toSceneSnapshot(p: ProtoSnapshot): SceneSnapshot {
  return {
    serverInfo: p.serverInfo ? toServerInfo(p.serverInfo) : undefined,
    stimuli: p.stimuli.map((s) => ({
      id: s.id,
      name: s.name,
      handle: s.handle,
      kind: kindOf(s.stimulusType),
      pos: { x: s.pos?.x ?? 0, y: s.pos?.y ?? 0 },
      orientation: s.orientation,
      opacity: s.opacity,
      fillColor: s.fillColor,
      enabled: s.enabled,
      drawOrder: s.drawOrder,
    })),
    frameCount: p.frameCount,
    serverTimeNs: p.serverTimeNs,
  };
}
