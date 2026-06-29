// Hand-written public domain types. These are the only types user code (and the
// React app) touches — the generated protobuf-es classes under src/_proto stay
// private, exactly as the Python client hides its _proto package.

/** A 2-D point in stimulus space: origin at screen centre, +x right, +y up, pixels. */
export interface Vec2 {
  x: number;
  y: number;
}

/** RGBA colour, each channel in [0, 1]. */
export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

export function rgb(r: number, g: number, b: number, a = 1): Color {
  return { r, g, b, a };
}

/** Opaque handle to a stimulus on the server. */
export type StimulusHandle = number;

/** Opaque handle to an animation on the server. */
export type AnimationHandle = number;

/** Stimulus kind, mirrors the server's StimulusType. */
export type StimulusKind =
  | "rect"
  | "circle"
  | "ellipse"
  | "grating"
  | "text"
  | "polygon"
  | "unknown";
