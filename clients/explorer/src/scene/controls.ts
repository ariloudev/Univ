// What the mouse does to the view (navigate-with-the-mouse), as pure functions of the view.

import { type View, clampDistance, clampPitch, eyeAxes, unitsPerPixel } from "./camera.ts";
import { type Mat3, type Vec3, add, scale, sub } from "./vec.ts";

/** Radians the view turns for each pixel dragged. */
const TURN_PER_PIXEL = 0.005;
/** The factor the distance to the focus changes by for each step of the wheel. */
export const STEP_FACTOR = 1.15;
/** The pixels a wheel's step scrolls, as browsers report a notch of a mouse wheel. */
const PIXELS_PER_STEP = 100;
/** A press that moves less than this, in pixels, is a click. */
export const CLICK_SLOP = 6;
/** How long a flight lasts, in milliseconds. */
export const FLIGHT_MS = 1200;

/** Turns the view about the focus for a drag of `dx`, `dy` pixels. */
export function turn(view: View, dx: number, dy: number): View {
  return { ...view, yaw: view.yaw - dx * TURN_PER_PIXEL, pitch: clampPitch(view.pitch + dy * TURN_PER_PIXEL) };
}

/** The steps a wheel event scrolled, from its delta and mode (pixels, lines or pages). */
export function wheelSteps(deltaY: number, deltaMode: number): number {
  const pixels = deltaMode === 1 ? deltaY * 33 : deltaMode === 2 ? deltaY * 800 : deltaY;
  return pixels / PIXELS_PER_STEP;
}

/** Moves the eye toward the focus for positive `steps`, away for negative ones. */
export function zoom(view: View, steps: number): View {
  return { ...view, distance: clampDistance(view.distance * STEP_FACTOR ** -steps) };
}

/** Moves the focus across the view for a drag of `dx`, `dy` pixels on a viewport `height` tall. */
export function pan(view: View, toFrame: Mat3, dx: number, dy: number, height: number): View {
  const axes = eyeAxes(view, toFrame);
  const units = unitsPerPixel(view, height);
  // The eye's right and up, back in the ICRS: the rows of the rotation into the eye's axes.
  const move = add(scale(axes[0], -dx * units), scale(axes[1], dy * units));
  return { ...view, focus: add(view.focus, move) };
}

/** Whether a press whose pointer travelled `distance` pixels in all is a click. */
export const isClick = (distance: number): boolean => distance < CLICK_SLOP;

/** The distance to view a thing `away` megaparsecs from Earth from, `size` across where known. */
export function viewingDistance(away: number, size?: number): number {
  if (size !== undefined && size > 0) return clampDistance(size * 2.5);
  return clampDistance(Math.max(away * 0.05, 1e-6));
}

/** The view `t` of the way (0 to 1) from `from` to `to`: the focus moved, the distance scaled,
 * the eye turned. */
export function between(from: View, to: View, t: number): View {
  const s = t * t * (3 - 2 * t);
  const focus = add(from.focus, scale(sub(to.focus, from.focus), s));
  const distance = from.distance * (to.distance / from.distance) ** s;
  // Turning the short way round: a flight that keeps the eye's direction does not turn at all.
  const turn = Math.atan2(Math.sin(to.yaw - from.yaw), Math.cos(to.yaw - from.yaw));
  return { focus, distance, yaw: from.yaw + turn * s, pitch: from.pitch + (to.pitch - from.pitch) * s };
}
