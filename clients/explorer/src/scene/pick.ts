// Which drawn point lies under the pointer (read-an-object): the nearest within a few pixels,
// the nearer to the eye where two are as near. Batches whose bounding sphere falls clear of
// the pointer are skipped whole, so that a pick reads only the tiles along its line of sight.

import { type View, eyeAxes, eyeOf, focalPixels } from "./camera.ts";
import type { Shown } from "./masks.ts";
import { type Mat3, type Vec3, apply, dot, norm, normalize, scale, sub } from "./vec.ts";

/** A batch of points to pick from: offsets from its centre, within `radius` of it. */
export interface Pickable {
  centre: Vec3;
  radius: number;
  offsets: Float32Array;
  /** Which points may be picked: every one where none is given. A point not drawn is passed
   * over, so that it never hides one drawn behind it. */
  shown?: Shown | undefined;
  /** How many of its points, the first, are drawn and may be picked: every one where none is
   * given. */
  count?: number | undefined;
}

/** The point picked: its batch and its index in the batch. */
export interface Picked {
  batch: number;
  index: number;
}

/** The pixels from the pointer within which a point is picked. */
export const PICK_SLOP = 6;

/** A viewport and the pointer on it, in CSS pixels. */
export interface Pointer {
  width: number;
  height: number;
  x: number;
  y: number;
}

interface Best extends Picked {
  d2: number;
  depth: number;
}

/** The point nearest the pointer within `PICK_SLOP` pixels, if any. */
export function pick(batches: readonly Pickable[], view: View, toFrame: Mat3, pointer: Pointer): Picked | undefined {
  const axes = eyeAxes(view, toFrame);
  const eye = eyeOf(view, toFrame);
  const f = focalPixels(pointer.height);
  let best: Best | undefined;
  batches.forEach((batch, index) => {
    best = pickIn(batch, index, axes, eye, f, pointer, best);
  });
  return best === undefined ? undefined : { batch: best.batch, index: best.index };
}

/** The pointer's ray from the eye, in the eye's axes. */
const rayOf = (f: number, pointer: Pointer): Vec3 =>
  normalize([(pointer.x - pointer.width / 2) / f, -(pointer.y - pointer.height / 2) / f, -1]);

/** Whether a sphere about `c` (in the eye's axes) of `radius`, widened by the pick's slop at its
 * depth, meets the pointer's `ray`: only then may it hold a point under the pointer. */
function mayHold(c: Vec3, radius: number, ray: Vec3, f: number): boolean {
  const along = dot(c, ray);
  if (along < -radius) return false;
  const across = norm(sub(c, scale(ray, along)));
  return across <= radius + (PICK_SLOP * Math.abs(along)) / f;
}

/** Points within a pixel of one another count as equally near the pointer. */
const TIE = 1;

/** Whether a point `d2` square pixels from the pointer at `depth` beats `found`: nearer the
 * pointer, or as near and nearer the eye. */
function better(d2: number, depth: number, found: Best | undefined): boolean {
  if (found === undefined || d2 + TIE < found.d2) return true;
  return Math.abs(d2 - found.d2) <= TIE && depth < found.depth;
}

/** Point `i` of offsets `o` from `centre` (both in the eye's `axes`) on the screen: its square
 * pixels from the pointer and its depth, if it is before the eye and within the pick's slop. */
function onScreen(o: Float32Array, i: number, centre: Vec3, axes: Mat3, f: number, pointer: Pointer): { d2: number; depth: number } | undefined {
  const [a, b, c] = axes;
  const ox = o[i * 3] ?? 0;
  const oy = o[i * 3 + 1] ?? 0;
  const oz = o[i * 3 + 2] ?? 0;
  const depth = -(centre[2] + c[0] * ox + c[1] * oy + c[2] * oz);
  if (depth <= 0) return undefined;
  const dx = pointer.width / 2 + ((centre[0] + a[0] * ox + a[1] * oy + a[2] * oz) * f) / depth - pointer.x;
  const dy = pointer.height / 2 - ((centre[1] + b[0] * ox + b[1] * oy + b[2] * oz) * f) / depth - pointer.y;
  const d2 = dx * dx + dy * dy;
  return d2 > PICK_SLOP * PICK_SLOP ? undefined : { d2, depth };
}

function pickIn(batch: Pickable, index: number, axes: Mat3, eye: Vec3, f: number, pointer: Pointer, best: Best | undefined): Best | undefined {
  const centre = apply(axes, sub(batch.centre, eye));
  if (batch.shown === "none" || !mayHold(centre, batch.radius, rayOf(f, pointer), f)) return best;
  const shown = typeof batch.shown === "function" ? batch.shown : () => true;
  let found = best;
  const count = Math.min(batch.count ?? Number.POSITIVE_INFINITY, batch.offsets.length / 3);
  for (let i = 0; i < count; i += 1) {
    const at = onScreen(batch.offsets, i, centre, axes, f, pointer);
    if (at !== undefined && shown(i) && better(at.d2, at.depth, found)) found = { batch: index, index: i, ...at };
  }
  return found;
}
