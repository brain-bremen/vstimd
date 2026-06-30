// Public API for the vstimd web client.
//
// Like the Python client, the generated protobuf-es stubs (src/_proto) are
// private implementation detail — user code imports only the hand-written
// domain types and clients re-exported here.

export { Connection } from "./connection.js";
export type { SnapshotListener, EventSubscription } from "./connection.js";

export { StimuliClient, ShapesClient } from "./stimuli.js";
export { GratingClient, type Waveform, type GratingMask } from "./grating.js";
export { TextClient } from "./text.js";
export { SystemClient, type ServerInfo } from "./system.js";
export { VtlClient, type VtlDirection, type VtlLine } from "./vtl.js";

export type { SceneSnapshot, StimulusView, VtlLineView } from "./snapshot.js";

export { rgb } from "./types.js";
export type {
  Vec2,
  Color,
  StimulusHandle,
  AnimationHandle,
  StimulusKind,
} from "./types.js";

export {
  ErrorCode,
  VstimdError,
  HandleNotFoundError,
  WrongStimulusTypeError,
  WrongTargetError,
  CreationFailedError,
  InvalidArgumentError,
  NotSupportedError,
  NotReadyError,
  UnknownServerError,
  ConfigNotFoundError,
  ConfigIoError,
  ConfigFormatError,
  ConfigVersionError,
  ConfigAlreadyExistsError,
} from "./errors.js";
export type { ServerResponse } from "./errors.js";
