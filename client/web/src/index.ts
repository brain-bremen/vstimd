// Public API for the vstimd web client.
//
// Like the Python client, the generated protobuf-es stubs (src/_proto) are
// private implementation detail — user code imports only the hand-written
// domain types and clients re-exported here.

export { Connection } from "./connection.js";
export type { SnapshotListener, EventSubscription } from "./connection.js";

export { StimuliClient, ShapesClient } from "./stimuli.js";
export { SystemClient, type ServerInfo } from "./system.js";

export type { SceneSnapshot, StimulusView } from "./snapshot.js";

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
