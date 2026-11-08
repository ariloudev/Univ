// The volume of a structure that the view can be culled to (see-the-structures): the solid of a
// sphere, an ellipsoid or an oriented box; a sky region's cells between its near and far
// distances; or, for a structure with no published volume, the box the view draws for it,
// marked approximate. Every volume is in ICRS megaparsecs, as the points are, and is placed as
// the view's outline is (structures.ts).

import type { Rotations } from "../api/schemas.ts";
import type { Geometry, StructureDetail } from "../api/structures.ts";
import type { Sphere } from "./frustum.ts";
import { cellAt, centreOf } from "./healpix.ts";
import { FINEST_ORDER, type Placing, type SkyRegion, halfExtents, placed, placingOf, primitiveAxes } from "./structures.ts";
import { type Mat3, type Vec3, add, scale, sub, transpose } from "./vec.ts";

/** An axis-aligned box in ICRS megaparsecs. */
export interface Box {
  min: Vec3;
  max: Vec3;
}

interface Common {
  /** Not the structure's published shape exactly: culled by a box, its bounds or its extents
   * laid about a point, and holding no point (`holds`). */
  approximate: boolean;
  /** The box that bounds the volume, which the tiles fetched must meet. */
  bounds: Box;
}

/** A ball or a cube about `centre`: inside where `toUnit` takes the offset from `centre` into
 * the unit ball, or the cube [-1, 1]³. */
interface Solid extends Common {
  kind: "solid";
  round: boolean;
  centre: Vec3;
  toUnit: Mat3;
}

/** A sky region: inside where the direction, in the region's frame, lies in one of its cells of
 * `order`, and the distance between `near` and `far`. */
interface Sky extends Common {
  kind: "sky";
  order: number;
  has: (cell: number) => boolean;
  toFrame: Mat3;
  near: number;
  far: number;
}

interface Aligned extends Common {
  kind: "box";
}

export type Volume = Solid | Sky | Aligned;

/** Nothing: no tile meets it and no point lies inside it, as the view stands while the volume it
 * is to be culled to is still being read. */
export const NOTHING: Volume = { kind: "box", approximate: true, bounds: { min: [1, 1, 1], max: [-1, -1, -1] } };

/** The inverse of `m`; none where it is singular. */
function inverse(m: Mat3): Mat3 | undefined {
  const [a, b, c] = m;
  const det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
  if (Math.abs(det) < 1e-300) return undefined;
  // The inverse's columns are the cross products of `m`'s rows, b × c, c × a and a × b, over
  // the determinant.
  const r = 1 / det;
  return [
    [(b[1] * c[2] - b[2] * c[1]) * r, (a[2] * c[1] - a[1] * c[2]) * r, (a[1] * b[2] - a[2] * b[1]) * r],
    [(b[2] * c[0] - b[0] * c[2]) * r, (a[0] * c[2] - a[2] * c[0]) * r, (a[2] * b[0] - a[0] * b[2]) * r],
    [(b[0] * c[1] - b[1] * c[0]) * r, (a[1] * c[0] - a[0] * c[1]) * r, (a[0] * b[1] - a[1] * b[0]) * r],
  ];
}

/** The solid about `centre` whose half-axes are `axes`, in ICRS megaparsecs. */
function solid(round: boolean, centre: Vec3, axes: readonly [Vec3, Vec3, Vec3], approximate: boolean): Solid | undefined {
  // The matrix whose columns are the half-axes takes the unit solid onto this one.
  const toUnit = inverse(transpose(axes));
  if (toUnit === undefined) return undefined;
  const [u, v, w] = axes;
  const reach = (i: 0 | 1 | 2) => (round ? Math.hypot(u[i], v[i], w[i]) : Math.abs(u[i]) + Math.abs(v[i]) + Math.abs(w[i]));
  const r: Vec3 = [reach(0), reach(1), reach(2)];
  return { kind: "solid", round, centre, toUnit, approximate, bounds: { min: sub(centre, r), max: add(centre, r) } };
}

/** The box bounding `points`; none for no points. */
function boxOf(points: readonly Vec3[]): Box | undefined {
  const [first, ...rest] = points;
  if (first === undefined) return undefined;
  return rest.reduce<Box>(
    ({ min, max }, p) => ({
      min: [Math.min(min[0], p[0]), Math.min(min[1], p[1]), Math.min(min[2], p[2])],
      max: [Math.max(max[0], p[0]), Math.max(max[1], p[1]), Math.max(max[2], p[2])],
    }),
    { min: first, max: first },
  );
}

const aligned = (bounds: Box): Aligned => ({ kind: "box", approximate: true, bounds });

/** The structure's bounding box, as the view draws it; none where it has none. */
function boundsBox(detail: StructureDetail): Aligned | undefined {
  const where = placed(detail);
  return where === undefined || detail.bounds === null ? undefined : aligned({ min: where.min, max: where.max });
}

/** The angle across a cell of `order`, radians, with room to spare: the cells' corners at the
 * near and far distances bound the region but for its edges' bulge, which this covers. */
const cellAngle = (order: number) => 2 * Math.sqrt((4 * Math.PI) / (12 * 4 ** order));

/** The highest order whose cells are looked up in a bitmap, of 12·4^order bytes: 12 MB. */
const BITMAP_ORDER = 10;

/** Whether a cell of `order` is one of `cells`. */
function cellSet(order: number, cells: readonly number[]): (cell: number) => boolean {
  if (order > BITMAP_ORDER) {
    const set = new Set(cells);
    return (cell) => set.has(cell);
  }
  const bitmap = new Uint8Array(12 * 4 ** order);
  for (const cell of cells) bitmap[cell] = 1;
  return (cell) => bitmap[cell] === 1;
}

/** A sky region's cells between its near and far distances; none where it has no cells. */
function skyVolume(region: SkyRegion, { turn, toFrame, k }: Placing): Volume | undefined {
  const near = region.near * k;
  const far = region.far * k;
  const directions = region.cells.map((cell) => turn(centreOf(region.order, cell)));
  const box = boxOf(directions.flatMap((d) => [scale(d, near), scale(d, far)]));
  if (box === undefined) return undefined;
  const pad = far * cellAngle(region.order);
  const bounds = { min: sub(box.min, [pad, pad, pad]), max: add(box.max, [pad, pad, pad]) };
  return { kind: "sky", order: region.order, has: cellSet(region.order, region.cells), toFrame, near, far, approximate: false, bounds };
}

/** The volume of `geometry`, placed by `placing`: the box of a point's published extents about
 * it, approximate; the structure's bounding box for a grid, or a sky region too fine to cull to;
 * none for a point without extents, a path, a ray or a range. */
function geometryVolume(geometry: Geometry, detail: StructureDetail, placing: Placing): Volume | undefined {
  switch (geometry.kind) {
    case "primitive": {
      const { round, centre, axes } = primitiveAxes(geometry.primitive, placing);
      return solid(round, centre, axes, false);
    }
    case "sky_region":
      return geometry.order > FINEST_ORDER ? boundsBox(detail) : skyVolume(geometry, placing);
    case "members": {
      const box = boxOf(geometry.members.map((m) => placing.place(m.at)));
      return box === undefined ? undefined : aligned(box);
    }
    case "point": {
      const axes = detail.derivation?.extents?.axes;
      return axes === undefined ? undefined : solid(false, placing.place(geometry.at), halfExtents(axes, placing.k), true);
    }
    case "volume_grid":
    case "grid_region":
      return boundsBox(detail);
    default:
      return undefined;
  }
}

/** The volume `detail` is culled to, its shape turned into the ICRS by `rotations`; none for a
 * structure drawn with no volume, as a point, a path, a ray or a range alone. A structure whose
 * shape cannot be placed is culled to its bounding box, as the view draws it. */
export function volumeOf(detail: StructureDetail, rotations: Rotations): Volume | undefined {
  const shape = detail.shape;
  const placing = shape === null ? undefined : placingOf(shape, rotations);
  return shape === null || placing === undefined ? boundsBox(detail) : geometryVolume(shape.geometry, detail, placing);
}

/** Whether a point at `x`, `y`, `z`, in ICRS megaparsecs, lies inside `volume`, its bounds
 * aside: worked out once for the volume, so that testing millions of points allocates nothing. */
function testerOf(volume: Volume): (x: number, y: number, z: number) => boolean {
  switch (volume.kind) {
    case "box":
      return () => true;
    case "solid": {
      const [[a0, a1, a2], [b0, b1, b2], [c0, c1, c2]] = volume.toUnit;
      const [cx, cy, cz] = volume.centre;
      const round = volume.round;
      return (x, y, z) => {
        const dx = x - cx;
        const dy = y - cy;
        const dz = z - cz;
        const u = a0 * dx + a1 * dy + a2 * dz;
        const v = b0 * dx + b1 * dy + b2 * dz;
        const w = c0 * dx + c1 * dy + c2 * dz;
        return round ? u * u + v * v + w * w <= 1 : Math.abs(u) <= 1 && Math.abs(v) <= 1 && Math.abs(w) <= 1;
      };
    }
    case "sky": {
      const [[a0, a1, a2], [b0, b1, b2], [c0, c1, c2]] = volume.toFrame;
      const { order, has, near, far } = volume;
      return (x, y, z) => {
        // A rotation keeps the distance, which is the direction's norm in either frame.
        const r = Math.sqrt(x * x + y * y + z * z);
        return r >= near && r <= far && has(cellAt(order, a0 * x + a1 * y + a2 * z, b0 * x + b1 * y + b2 * z, c0 * x + c1 * y + c2 * z, r));
      };
    }
  }
}

/** Whether `p`, in ICRS megaparsecs, lies within the box `box`. */
const within = ({ min, max }: Box, x: number, y: number, z: number): boolean =>
  x >= min[0] && x <= max[0] && y >= min[1] && y <= max[1] && z >= min[2] && z <= max[2];

/** Whether the point at `x`, `y`, `z`, in ICRS megaparsecs, lies inside `volume`. */
export const inside = (volume: Volume, x: number, y: number, z: number): boolean => within(volume.bounds, x, y, z) && testerOf(volume)(x, y, z);

/** Whether the structure whose volume is `volume` holds the point at `place`, ICRS megaparsecs:
 * only a volume its shape gives exactly holds one. An approximate volume, which the view culls to,
 * holds none: a bounding box, or a void's published extents laid about its deepest point where
 * its paper gives no centre, so that where it ends on each side is unknown. */
export const holds = (volume: Volume, place: Vec3): boolean => !volume.approximate && inside(volume, place[0], place[1], place[2]);

/** Whether the sphere `extent` meets the box `box`. */
function sphereMeets({ min, max }: Box, { centre, radius }: Sphere): boolean {
  const gap = (i: 0 | 1 | 2) => Math.max(min[i] - centre[i], 0, centre[i] - max[i]);
  return Math.hypot(gap(0), gap(1), gap(2)) <= radius;
}

/** Which of the points at `offsets` from `centre`, all within the sphere `extent`, lie inside
 * `volume`: 1 inside, 0 outside. None is tested where `extent` misses the volume's bounds. */
export function insideOf(volume: Volume, centre: Vec3, offsets: Float32Array, extent: Sphere): Uint8Array {
  const mask = new Uint8Array(offsets.length / 3);
  if (!sphereMeets(volume.bounds, extent)) return mask;
  const test = testerOf(volume);
  const [cx, cy, cz] = centre;
  for (let i = 0; i < mask.length; i += 1) {
    const x = cx + (offsets[3 * i] ?? 0);
    const y = cy + (offsets[3 * i + 1] ?? 0);
    const z = cz + (offsets[3 * i + 2] ?? 0);
    mask[i] = within(volume.bounds, x, y, z) && test(x, y, z) ? 1 : 0;
  }
  return mask;
}

/** Whether the cube about `centre` with edges `edge` long meets the box `box`. */
export function meets({ min, max }: Box, centre: Vec3, edge: number): boolean {
  const h = edge / 2;
  return (
    centre[0] + h >= min[0] && centre[0] - h <= max[0] && centre[1] + h >= min[1] && centre[1] - h <= max[1] && centre[2] + h >= min[2] && centre[2] - h <= max[2]
  );
}
