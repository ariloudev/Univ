// Which drawn point lies under the pointer (read-an-object): the nearest within a few pixels,
// the nearer to the eye where two are as near. Batches whose bounding sphere falls clear of
// the pointer are skipped whole, so that a pick reads only the tiles along its line of sight.

import { FOV, type View, eyeAxes, eyeOf } from "./camera.ts";
import type { Mat3, Vec3 } from "./vec.ts";

/** A batch of points to pick from: offsets from its centre, within `radius` of it. */
export interface Pickable {
  centre: Vec3;
  radius: number;
  offsets: Float32Array;
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
  const f = pointer.height / 2 / Math.tan(FOV / 2);
  let best: Best | undefined;
  batches.forEach((batch, index) => {
    best = pickIn(batch, index, axes, eye, f, pointer, best);
  });
  return best === undefined ? undefined : { batch: best.batch, index: best.index };
}

/** Whether `batch`'s bounding sphere, seen from the eye, may hold a point under the pointer. */
function mayHold(c: Vec3, radius: number, f: number, pointer: Pointer): boolean {
  const depth = -c[2];
  if (depth + radius <= 0) return false;
  if (depth <= radius) return true;
  const x = pointer.width / 2 + (c[0] * f) / depth;
  const y = pointer.height / 2 - (c[1] * f) / depth;
  return Math.hypot(x - pointer.x, y - pointer.y) <= (radius * f) / (depth - radius) + PICK_SLOP;
}

/** Points within a pixel of one another count as equally near the pointer. */
const TIE = 1;

/** Whether a point `d2` square pixels from the pointer at `depth` beats `found`: nearer the
 * pointer, or as near and nearer the eye. */
function better(d2: number, depth: number, found: Best | undefined): boolean {
  if (found === undefined || d2 + TIE < found.d2) return true;
  return Math.abs(d2 - found.d2) <= TIE && depth < found.depth;
}

function pickIn(batch: Pickable, index: number, axes: Mat3, eye: Vec3, f: number, pointer: Pointer, best: Best | undefined): Best | undefined {
  const [a, b, c] = axes;
  const d: Vec3 = [batch.centre[0] - eye[0], batch.centre[1] - eye[1], batch.centre[2] - eye[2]];
  const centre: Vec3 = [a[0] * d[0] + a[1] * d[1] + a[2] * d[2], b[0] * d[0] + b[1] * d[1] + b[2] * d[2], c[0] * d[0] + c[1] * d[1] + c[2] * d[2]];
  if (!mayHold(centre, batch.radius, f, pointer)) return best;
  let found = best;
  const o = batch.offsets;
  for (let i = 0; i < o.length / 3; i += 1) {
    const ox = o[i * 3] ?? 0;
    const oy = o[i * 3 + 1] ?? 0;
    const oz = o[i * 3 + 2] ?? 0;
    const depth = -(centre[2] + c[0] * ox + c[1] * oy + c[2] * oz);
    if (depth <= 0) continue;
    const dx = pointer.width / 2 + ((centre[0] + a[0] * ox + a[1] * oy + a[2] * oz) * f) / depth - pointer.x;
    const dy = pointer.height / 2 - ((centre[1] + b[0] * ox + b[1] * oy + b[2] * oz) * f) / depth - pointer.y;
    const d2 = dx * dx + dy * dy;
    if (d2 > PICK_SLOP * PICK_SLOP) continue;
    if (better(d2, depth, found)) found = { batch: index, index: i, d2, depth };
  }
  return found;
}
