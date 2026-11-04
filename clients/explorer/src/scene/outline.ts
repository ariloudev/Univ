// Outlines of placed shapes (see-the-structures): spheres, ellipsoids and oriented boxes as
// rings and edges, paths, rays and ranges as lines, points and member lists as markers. Every
// position is ICRS megaparsecs once `place` has turned it from the shape's frame and unit.

import { type Vec3, add, scale } from "./vec.ts";

/** Segments of each ring of a sphere or an ellipsoid. */
const SEGMENTS = 96;

/** What a shape is drawn as: segments as pairs of points, and markers. */
export interface Outline {
  segments: Vec3[];
  markers: Vec3[];
}

export const empty = (): Outline => ({ segments: [], markers: [] });

/** The ring through `centre + u cos t + v sin t`. */
function ring(centre: Vec3, u: Vec3, v: Vec3): Vec3[] {
  const at = (t: number) => add(centre, add(scale(u, Math.cos(t)), scale(v, Math.sin(t))));
  const segments: Vec3[] = [];
  for (let i = 0; i < SEGMENTS; i += 1) {
    segments.push(at((2 * Math.PI * i) / SEGMENTS), at((2 * Math.PI * (i + 1)) / SEGMENTS));
  }
  return segments;
}

/** Three rings in the planes of `axes`, each axis already scaled to its semi-axis. */
export function rings(centre: Vec3, axes: readonly [Vec3, Vec3, Vec3]): Vec3[] {
  const [a, b, c] = axes;
  return [...ring(centre, a, b), ...ring(centre, b, c), ...ring(centre, c, a)];
}

/** The twelve edges of the box about `centre` with half-extents along `axes`. */
export function boxEdges(centre: Vec3, axes: readonly [Vec3, Vec3, Vec3]): Vec3[] {
  const corner = (i: number) =>
    axes.reduce<Vec3>((p, axis, k) => add(p, scale(axis, (i >> k) & 1 ? 1 : -1)), centre);
  const segments: Vec3[] = [];
  for (let i = 0; i < 8; i += 1) {
    for (let k = 0; k < 3; k += 1) {
      if (((i >> k) & 1) === 0) segments.push(corner(i), corner(i | (1 << k)));
    }
  }
  return segments;
}

/** The segments of a polyline through `points`. */
export function polyline(points: readonly Vec3[]): Vec3[] {
  return points.slice(1).flatMap((point, i) => [points[i] ?? point, point]);
}

/** The bounding box `min` to `max` as its twelve edges. */
export function boundsEdges(min: Vec3, max: Vec3): Vec3[] {
  const centre = scale(add(min, max), 0.5);
  const half = scale(add(max, scale(min, -1)), 0.5);
  return boxEdges(centre, [
    [half[0], 0, 0],
    [0, half[1], 0],
    [0, 0, half[2]],
  ]);
}
