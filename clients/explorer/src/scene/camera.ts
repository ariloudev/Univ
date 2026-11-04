// The view (navigate-with-the-mouse): an eye turning about a focus, in the axes of a frame.
//
// Positions are ICRS Cartesian megaparsecs about the Solar System barycentre, as the tiles give
// them once converted to megaparsecs. The view keeps its focus in the ICRS and its eye's
// direction in the chosen frame, whose pole is up: changing the frame turns the view and moves
// no point. Everything is in double precision here; the renderer receives positions relative
// to the eye, so that float32 keeps parsec detail a hundred megaparsecs out.

import { type Mat3, type Vec3, add, apply, cross, multiply, normalize, scale, sub, unapply } from "./vec.ts";

/** The nearest the eye comes to the focus: 0.1 parsec, in megaparsecs. */
export const MIN_DISTANCE = 1e-7;
/** The farthest the eye goes from the focus: 20 gigaparsecs, in megaparsecs. */
export const MAX_DISTANCE = 2e4;
/** The vertical field of view, in radians. */
export const FOV = (50 * Math.PI) / 180;
/** The tilt stops this short of either pole, in radians. */
const POLE_MARGIN = 0.02;
export const MAX_PITCH = Math.PI / 2 - POLE_MARGIN;

export interface View {
  /** The focus, ICRS megaparsecs. */
  focus: Vec3;
  /** The distance from the eye to the focus, megaparsecs. */
  distance: number;
  /** The eye's longitude about the frame's pole, radians. */
  yaw: number;
  /** The eye's latitude above the frame's equator, radians. */
  pitch: number;
}

export const clampDistance = (d: number): number => Math.min(MAX_DISTANCE, Math.max(MIN_DISTANCE, d));
export const clampPitch = (p: number): number => Math.min(MAX_PITCH, Math.max(-MAX_PITCH, p));

/** The direction from the focus to the eye, in the frame's axes. */
function outward(view: View): Vec3 {
  const c = Math.cos(view.pitch);
  return [c * Math.cos(view.yaw), c * Math.sin(view.yaw), Math.sin(view.pitch)];
}

/** The eye, ICRS megaparsecs; `toFrame` turns the ICRS into the frame's axes. */
export function eyeOf(view: View, toFrame: Mat3): Vec3 {
  return add(view.focus, unapply(toFrame, scale(outward(view), view.distance)));
}

/** The rotation from the ICRS to the eye's axes: right, up, and backward (OpenGL's camera). */
export function eyeAxes(view: View, toFrame: Mat3): Mat3 {
  const back = outward(view);
  const right = normalize(cross([0, 0, 1], back));
  const up = cross(back, right);
  return multiply([right, up, back], toFrame);
}

/** The focal length in pixels of a viewport `height` pixels tall. */
export const focalPixels = (height: number): number => height / 2 / Math.tan(FOV / 2);

/** Where `point` (ICRS megaparsecs) falls on a viewport `width` × `height` pixels; none behind. */
export function project(
  view: View,
  toFrame: Mat3,
  point: Vec3,
  width: number,
  height: number,
): { x: number; y: number; depth: number } | undefined {
  const local = apply(eyeAxes(view, toFrame), sub(point, eyeOf(view, toFrame)));
  const depth = -local[2];
  if (depth <= 0) return undefined;
  const f = focalPixels(height);
  return { x: width / 2 + (local[0] * f) / depth, y: height / 2 - (local[1] * f) / depth, depth };
}

/** The megaparsecs one pixel spans at the focus's depth, on a viewport `height` pixels tall. */
export function unitsPerPixel(view: View, height: number): number {
  return (2 * view.distance * Math.tan(FOV / 2)) / height;
}
