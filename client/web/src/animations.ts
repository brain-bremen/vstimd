// Frame-accurate animation client. Mirrors vstimd.animations (Python).
//
// Animations run once per frame in the render loop. They are created IDLE and
// must be armed before they fire. Frame/time parameters accept either a
// `*Frames` integer or a `*Ms` float (converted using the server frame rate,
// queried lazily and cached). Proto types stay private: the public surface uses
// string-union enums (state/edge/actions) and the `VtlHandle` addressing type.

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { RequestSchema } from "./_proto/vstimd/v1/service_pb.js";
import {
  AnimationState as ProtoState,
  CreateAnimationRequestSchema,
} from "./_proto/vstimd/v1/animations_pb.js";
import { VtlEdge as ProtoEdge } from "./_proto/vstimd/v1/vtl_pb.js";
import { vtlHandleProto, type VtlHandle } from "./vtl.js";
import type { Send } from "./transport.js";
import type { AnimationHandle, StimulusHandle } from "./types.js";

/** Lifecycle state of an animation. */
export type AnimationState = "idle" | "armed" | "running" | "done";

/** Which trigger edge an animation reacts to. */
export type VtlEdge = "rising" | "falling";

/** Actions performed when an animation starts. */
export type StartAction = "enable" | "togglePhotodiode" | "startActionTriggerLine";

/** Actions performed when an animation completes. */
export type FinalAction =
  | "disable"
  | "togglePhotodiode"
  | "finalActionTriggerLine"
  | "restart"
  | "reverse"
  | "restoreState"
  | "endDeferred";

/** Actions performed when an animation is cancelled (edge or software). */
export type CancelAction =
  | "disable"
  | "togglePhotodiode"
  | "cancelActionTriggerLine"
  | "restoreState"
  | "endDeferred";

/**
 * Animation-type tag. This is the server's canonical `type_name` — the Rust enum
 * variant name, which is also the serde tag written to config files. The server
 * sends it verbatim in both `list()` and `query()`, so the client never derives
 * or normalizes it; this union just mirrors the server set for autocomplete.
 */
export type AnimationTypeName =
  | "CoupleVisibilityToTriggerLine"
  | "EnableOnTriggerEdge"
  | "FlashForNFrames"
  | "FlickerForNFrames"
  | "MoveAlongPath2D"
  | "MoveAlongSegments2D"
  | "ExternalPosition2D";

/** One animation as returned by `list()`. */
export interface AnimationInfo {
  handle: AnimationHandle;
  name: string;
  state: AnimationState;
  typeName: AnimationTypeName;
}

/** Full configuration + state as returned by `query()`. */
export interface AnimationDetails extends AnimationInfo {
  stimuli: StimulusHandle[];
  finalActions: FinalAction[];
  cancelActions: CancelAction[];
}

/** One or more stimuli an animation drives. */
export type Stimuli = StimulusHandle | StimulusHandle[];

/** Trigger/action wiring shared by every `create*` call. */
export interface AnimationOpts {
  name?: string;
  startActions?: StartAction[];
  startActionTriggerLine?: VtlHandle;
  finalActions?: FinalAction[];
  finalActionTriggerLine?: VtlHandle;
  /** Wait for this line's edge after arming before starting. */
  startTrigger?: VtlHandle;
  startEdge?: VtlEdge;
  /** Cancel when this line's edge fires while Armed or Running. */
  cancelTrigger?: VtlHandle;
  cancelEdge?: VtlEdge;
  /** Actions applied on cancel (edge or software). Empty = hard abort. */
  cancelActions?: CancelAction[];
  cancelActionTriggerLine?: VtlHandle;
}

const EDGE: Record<VtlEdge, ProtoEdge> = {
  rising: ProtoEdge.RISING,
  falling: ProtoEdge.FALLING,
};

const START_ACTION: Record<StartAction, number> = {
  enable: 0x02,
  togglePhotodiode: 0x04,
  startActionTriggerLine: 0x08,
};

const FINAL_ACTION: Record<FinalAction, number> = {
  disable: 0x01,
  togglePhotodiode: 0x04,
  finalActionTriggerLine: 0x08,
  restart: 0x10,
  reverse: 0x20,
  restoreState: 0x40,
  endDeferred: 0x80,
};

const CANCEL_ACTION: Record<CancelAction, number> = {
  disable: 0x01,
  togglePhotodiode: 0x04,
  cancelActionTriggerLine: 0x08,
  restoreState: 0x40,
  endDeferred: 0x80,
};


function stateOf(s: ProtoState): AnimationState {
  switch (s) {
    case ProtoState.ARMED: return "armed";
    case ProtoState.RUNNING: return "running";
    case ProtoState.DONE: return "done";
    default: return "idle";
  }
}

function maskOf<T extends string>(flags: T[] | undefined, table: Record<T, number>): number {
  return (flags ?? []).reduce((m, f) => m | table[f], 0);
}

function decodeFinalActions(mask: number): FinalAction[] {
  return (Object.keys(FINAL_ACTION) as FinalAction[]).filter((f) => (mask & FINAL_ACTION[f]) !== 0);
}

function decodeCancelActions(mask: number): CancelAction[] {
  return (Object.keys(CANCEL_ACTION) as CancelAction[]).filter((f) => (mask & CANCEL_ACTION[f]) !== 0);
}

function toStimuli(s: Stimuli): StimulusHandle[] {
  return typeof s === "number" ? [s] : s;
}

// The animation-type oneof inside CreateAnimationRequest.
type AnimBody = NonNullable<MessageInitShape<typeof CreateAnimationRequestSchema>["body"]>;

export class AnimationsClient {
  private fpsCache?: number;

  constructor(
    private readonly send: Send,
    private readonly fpsGetter: () => Promise<number>,
  ) {}

  /** Server frame rate, queried once and cached. */
  private async fps(): Promise<number> {
    if (this.fpsCache === undefined) this.fpsCache = await this.fpsGetter();
    return this.fpsCache;
  }

  /** Invalidate the cached frame rate so it is re-queried on next ms conversion. */
  refreshFps(): void {
    this.fpsCache = undefined;
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  /** Arm an animation (IDLE → ARMED, or RUNNING for free-running types). */
  async arm(handle: AnimationHandle): Promise<void> {
    await this.system({ case: "armAnimation", value: { handle } });
  }

  /** Disarm an animation (back to IDLE). */
  async disarm(handle: AnimationHandle): Promise<void> {
    await this.system({ case: "disarmAnimation", value: { handle } });
  }

  /**
   * Cancel an animation with a clean teardown (ends in DONE). Unlike `disarm`
   * (which just returns to IDLE), cancel runs the animation's final action —
   * leaving visibility in a defined state, pulsing any trigger line, and
   * releasing the hold. `restart` is not honored. Works while ARMED or RUNNING.
   */
  async cancel(handle: AnimationHandle): Promise<void> {
    await this.system({ case: "cancelAnimation", value: { handle } });
  }

  async delete(handle: AnimationHandle): Promise<void> {
    await this.system({ case: "deleteAnimation", value: { handle } });
  }

  /** List all animations and their current state. */
  async list(): Promise<AnimationInfo[]> {
    const resp = await this.system({ case: "listAnimations", value: {} });
    const anims = resp.body.case === "animationList" ? resp.body.value.animations : [];
    return anims.map((a) => ({
      handle: a.handle,
      name: a.name,
      state: stateOf(a.state),
      typeName: a.typeName as AnimationTypeName,
    }));
  }

  /** Return the full configuration and current state of an animation. */
  async query(handle: AnimationHandle): Promise<AnimationDetails> {
    const resp = await this.system({ case: "queryAnimation", value: { handle } });
    if (resp.body.case !== "queryAnimationResponse") {
      throw new Error("unexpected response to queryAnimation");
    }
    const r = resp.body.value;
    const p = r.params;
    return {
      handle: r.handle,
      name: p?.name ?? "",
      state: stateOf(r.state),
      typeName: r.typeName as AnimationTypeName,
      stimuli: p?.stimuli ?? [],
      finalActions: decodeFinalActions(p?.finalActionMask ?? 0),
      cancelActions: decodeCancelActions(p?.cancelActionMask ?? 0),
    };
  }

  // ── Animation types ──────────────────────────────────────────────────────

  /** Mirror stimulus enabled state to the level of a trigger line. */
  coupleVisibilityToTriggerLine(
    trigger: VtlHandle,
    stimuli: Stimuli,
    opts: { polarity?: boolean } & AnimationOpts = {},
  ): Promise<AnimationHandle> {
    return this.create(stimuli, opts, {
      case: "coupleVisibilityToTriggerLine",
      value: { trigger: vtlHandleProto(trigger), polarity: opts.polarity ?? true },
    });
  }

  /** Set stimulus enabled once when a trigger edge fires. */
  enableOnTriggerEdge(
    trigger: VtlHandle,
    stimuli: Stimuli,
    opts: { edge?: VtlEdge; enabled?: boolean } & AnimationOpts = {},
  ): Promise<AnimationHandle> {
    return this.create(stimuli, opts, {
      case: "enableOnTriggerEdge",
      value: { trigger: vtlHandleProto(trigger), edge: EDGE[opts.edge ?? "rising"], enabled: opts.enabled ?? true },
    });
  }

  /** Enable stimuli for a fixed duration. Give `durationFrames` or `durationMs`. */
  async flash(
    stimuli: Stimuli,
    opts: { durationFrames?: number; durationMs?: number } & AnimationOpts = {},
  ): Promise<AnimationHandle> {
    const durationFrames = await this.toFrames(opts.durationFrames, opts.durationMs, "duration");
    return this.create(stimuli, opts, { case: "flashForNFrames", value: { durationFrames } });
  }

  /** Flicker stimuli on/off. Omit `total*` to run forever. */
  async flicker(
    stimuli: Stimuli,
    opts: {
      onFrames?: number; onMs?: number;
      offFrames?: number; offMs?: number;
      totalFrames?: number; totalMs?: number;
      startOnPhase?: boolean;
    } & AnimationOpts = {},
  ): Promise<AnimationHandle> {
    const onFrames = await this.toFrames(opts.onFrames, opts.onMs, "on");
    const offFrames = await this.toFrames(opts.offFrames, opts.offMs, "off");
    const hasTotal = opts.totalFrames !== undefined || opts.totalMs !== undefined;
    const totalFrames = hasTotal ? await this.toFrames(opts.totalFrames, opts.totalMs, "total") : undefined;
    return this.create(stimuli, opts, {
      case: "flickerForNFrames",
      value: { onFrames, offFrames, totalFrames, startOnPhase: opts.startOnPhase ?? true },
    });
  }

  /** Move a stimulus through one position per frame. `x`/`y` must be equal length. */
  moveAlongPath2d(
    stimuli: Stimuli,
    x: number[],
    y: number[],
    opts: AnimationOpts = {},
  ): Promise<AnimationHandle> {
    if (x.length !== y.length) throw new Error("x and y must have equal length");
    return this.create(stimuli, opts, { case: "moveAlongPath2d", value: { x, y } });
  }

  /** Move a stimulus along piecewise-linear waypoints at constant speed. */
  moveAlongSegments2d(
    stimuli: Stimuli,
    x: number[],
    y: number[],
    speedPxPerSec: number,
    opts: AnimationOpts = {},
  ): Promise<AnimationHandle> {
    if (x.length !== y.length) throw new Error("x and y must have equal length");
    if (x.length < 2) throw new Error("at least 2 waypoints required");
    return this.create(stimuli, opts, { case: "moveAlongSegments2d", value: { x, y, speedPxPerSec } });
  }

  /** Read stimulus position from a POSIX shared-memory float array each frame. */
  externalPosition2d(
    stimuli: Stimuli,
    shmName: string,
    opts: { xOffset?: number; yOffset?: number } & AnimationOpts = {},
  ): Promise<AnimationHandle> {
    return this.create(stimuli, opts, {
      case: "externalPosition2d",
      value: { shmName, xOffset: opts.xOffset ?? 0, yOffset: opts.yOffset ?? 0 },
    });
  }

  // ── Internals ────────────────────────────────────────────────────────────

  private async toFrames(frames: number | undefined, ms: number | undefined, param: string): Promise<number> {
    if (frames !== undefined && ms !== undefined) {
      throw new Error(`specify either ${param}Frames or ${param}Ms, not both`);
    }
    if (frames !== undefined) return frames;
    if (ms !== undefined) return Math.max(1, Math.ceil((ms / 1000) * (await this.fps())));
    throw new Error(`one of ${param}Frames or ${param}Ms must be specified`);
  }

  // Build the shared CreateAnimationRequest fields from AnimationOpts.
  private base(stimuli: Stimuli, opts: AnimationOpts) {
    return {
      name: opts.name ?? "",
      startActionMask: maskOf(opts.startActions, START_ACTION),
      startActionTriggerLine: opts.startActionTriggerLine ? vtlHandleProto(opts.startActionTriggerLine) : undefined,
      finalActionMask: maskOf(opts.finalActions, FINAL_ACTION),
      finalActionTriggerLine: opts.finalActionTriggerLine ? vtlHandleProto(opts.finalActionTriggerLine) : undefined,
      startTrigger: opts.startTrigger ? vtlHandleProto(opts.startTrigger) : undefined,
      startEdge: EDGE[opts.startEdge ?? "rising"],
      cancelTrigger: opts.cancelTrigger ? vtlHandleProto(opts.cancelTrigger) : undefined,
      cancelEdge: EDGE[opts.cancelEdge ?? "rising"],
      cancelActionMask: maskOf(opts.cancelActions, CANCEL_ACTION),
      cancelActionTriggerLine: opts.cancelActionTriggerLine ? vtlHandleProto(opts.cancelActionTriggerLine) : undefined,
      stimuli: toStimuli(stimuli),
    };
  }

  private async create(stimuli: Stimuli, opts: AnimationOpts, body: AnimBody): Promise<AnimationHandle> {
    const resp = await this.send(
      create(RequestSchema, {
        target: { case: "system", value: {} },
        body: { case: "createAnimation", value: { ...this.base(stimuli, opts), body } },
      }),
    );
    return resp.handle;
  }

  private system(body: MessageInitShape<typeof RequestSchema>["body"]) {
    return this.send(create(RequestSchema, { target: { case: "system", value: {} }, body }));
  }
}
