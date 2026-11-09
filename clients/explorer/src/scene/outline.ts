// Outlines of placed shapes (see-the-structures): spheres, ellipsoids and oriented boxes as
// rings and edges, paths, rays and ranges as lines, points and member lists as markers. Every
// position is ICRS megaparsecs once `place` has turned it from the shape's frame and unit.

import type { Flow } from "./kinds.ts";
import { ORIGIN, type Vec3, add, scale, sub } from "./vec.ts";

/** Segments of each ring of a sphere or an ellipsoid. */
const SEGMENTS = 96;

/** What a shape is drawn as: segments as pairs of points, and markers. */
export interface Outline {
  segments: Vec3[];
  markers: Vec3[];
}

export const empty = (): Outline => ({ segments: [], markers: [] });

/** An outline of `segments` alone. */
export const lines = (segments: Vec3[]): Outline => ({ segments, markers: [] });

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
  const half = scale(sub(max, min), 0.5);
  return boxEdges(centre, [
    [half[0], 0, 0],
    [0, half[1], 0],
    [0, 0, half[2]],
  ]);
}

/** Where an arrow of the flow symbol starts and ends, in its sphere's radii from its centre, and
 * how long and wide its head is. */
const ARROW_NEAR = 1.15;
/** How far the flow symbol's arrows reach, in its sphere's radii. */
export const FLOW_REACH = 2;
const HEAD_BACK = 0.3;
const HEAD_ACROSS = 0.15;

/** The segments of an arrow from `tail` to `tip`, pointing along the unit vector `towards`, its
 * head four barbs back from the tip, across along `u` and along `v`. */
function arrow(tail: Vec3, tip: Vec3, towards: Vec3, u: Vec3, v: Vec3): Vec3[] {
  const back = scale(towards, -HEAD_BACK);
  const barbs = [u, scale(u, -1), v, scale(v, -1)].flatMap((across) => [tip, add(add(tip, back), scale(across, HEAD_ACROSS))]);
  return [tail, tip, ...barbs];
}

/** The symbol of a centre of a flow (see-the-structures), about the origin with a radius of one:
 * a sphere as three rings in the planes of `axes`, unit vectors, and an arrow along each axis
 * either way, pointing in toward the centre where the flow runs `in`, and out where it runs
 * `out`. */
export function flowSymbol(flow: Flow, axes: readonly [Vec3, Vec3, Vec3]): Vec3[] {
  const arrows = axes.flatMap((axis, k) => {
    const u = axes[(k + 1) % 3] ?? axis;
    const v = axes[(k + 2) % 3] ?? axis;
    return [axis, scale(axis, -1)].flatMap((way) => {
      const near = scale(way, ARROW_NEAR);
      const far = scale(way, FLOW_REACH);
      return flow === "in" ? arrow(far, near, scale(way, -1), u, v) : arrow(near, far, way, u, v);
    });
  });
  return [...rings(ORIGIN, axes), ...arrows];
}
