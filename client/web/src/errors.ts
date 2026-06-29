// Public error types and result codes — mirrors the Python client's response.py
// and exceptions.py. Maps the server's ErrorCode onto typed errors so callers
// never inspect a raw proto enum.

/** Machine-readable result codes (mirrors proto ErrorCode). */
export enum ErrorCode {
  UNSPECIFIED = 0,
  OK = 1,
  UNKNOWN = 2,
  HANDLE_NOT_FOUND = 3,
  WRONG_STIMULUS_TYPE = 4,
  WRONG_TARGET = 5,
  CREATION_FAILED = 6,
  INVALID_ARGUMENT = 7,
  NOT_SUPPORTED = 8,
  NOT_READY = 9,
  FILE_NOT_FOUND = 10,
  FILE_IO = 11,
  FILE_FORMAT = 12,
  UNSUPPORTED_VERSION = 13,
  FILE_ALREADY_EXISTS = 14,
}

/** Envelope returned by every mutation command (mirrors ServerResponse). */
export interface ServerResponse {
  /** New handle on `create*`; -1 on mutations/deletes. */
  handle: number;
  /** Stable UUID of a newly created stimulus; "" otherwise. */
  id: string;
  /** Render frames completed at response time. */
  frameCount: bigint;
  /** Nanoseconds since server start (monotonic). */
  serverTimeNs: bigint;
}

export class VstimdError extends Error {
  readonly code: ErrorCode;
  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = new.target.name;
    this.code = code;
  }
}

export class HandleNotFoundError extends VstimdError {}
export class WrongStimulusTypeError extends VstimdError {}
export class WrongTargetError extends VstimdError {}
export class CreationFailedError extends VstimdError {}
export class InvalidArgumentError extends VstimdError {}
export class NotSupportedError extends VstimdError {}
export class NotReadyError extends VstimdError {}
export class UnknownServerError extends VstimdError {}
export class ConfigNotFoundError extends VstimdError {}
export class ConfigIoError extends VstimdError {}
export class ConfigFormatError extends VstimdError {}
export class ConfigVersionError extends VstimdError {}
export class ConfigAlreadyExistsError extends VstimdError {}

const ERROR_CTORS: Partial<Record<ErrorCode, new (c: ErrorCode, m: string) => VstimdError>> = {
  [ErrorCode.UNKNOWN]: UnknownServerError,
  [ErrorCode.HANDLE_NOT_FOUND]: HandleNotFoundError,
  [ErrorCode.WRONG_STIMULUS_TYPE]: WrongStimulusTypeError,
  [ErrorCode.WRONG_TARGET]: WrongTargetError,
  [ErrorCode.CREATION_FAILED]: CreationFailedError,
  [ErrorCode.INVALID_ARGUMENT]: InvalidArgumentError,
  [ErrorCode.NOT_SUPPORTED]: NotSupportedError,
  [ErrorCode.NOT_READY]: NotReadyError,
  [ErrorCode.FILE_NOT_FOUND]: ConfigNotFoundError,
  [ErrorCode.FILE_IO]: ConfigIoError,
  [ErrorCode.FILE_FORMAT]: ConfigFormatError,
  [ErrorCode.UNSUPPORTED_VERSION]: ConfigVersionError,
  [ErrorCode.FILE_ALREADY_EXISTS]: ConfigAlreadyExistsError,
}

/** Throw the typed error for a non-OK code; no-op on OK. */
export function throwForCode(code: ErrorCode, message: string): void {
  if (code === ErrorCode.OK) return;
  const Ctor = ERROR_CTORS[code] ?? UnknownServerError;
  throw new Ctor(code, message || `server error code ${code}`);
}
