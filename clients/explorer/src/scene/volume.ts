// The volume of a structure that the view can be culled to (see-the-structures): the solid of a
// sphere, an ellipsoid or an oriented box; a sky region's cells between its near and far
// distances; or, for a structure with no published volume, the box the view draws for it,
// marked approximate. Every volume is in ICRS megaparsecs, as the points are.

import type { Rotations } from "../api/schemas.ts";
import type { StructureDetail } from "../api/structures.ts";
import { MAX_CELL_ORDER, cellOf, centreOf } from "./healpix.ts";
import { toFrameOf } from "./structures.ts";
import { toMpc } from "./units.ts";
import { type Mat3, type Vec3, add, apply, scale, sub, transpose, unapply } from "./vec.ts";

/** An axis-aligned box in ICRS megaparsecs. */
export interface Box {
  min: Vec3;
  max: Vec3;
}

interface Common {
  /** Culled by a box that is not the structure's published shape. */
  approximate: boolean;
  /** The box that bounds the volume, which the tiles fetched must meet. */
  bounds: Box;
}

/** A ball or a cube about `centre`: inside where `toUnit` takes the offset from `centre` into
 * the unit ball, or the cube [-1, 1]³. */
interface Solid extends Common {
  kind: "solid";
  form: "ball" | "cube";
  centre: Vec3;
  toUnit: Mat3;
}

/** A sky region: inside where the direction, in the region's frame, lies in one of its cells of
 * `order`, and the distance between `near` and `far`. */
interface Sky extends Common {
  kind: "sky";
  order: number;
  cells: ReadonlySet<number>;
  toFrame: Mat3;
  near: number;
  far: number;
}

interface Aligned extends Common {
  kind: "box";
}

export type Volume = Solid | Sky | Aligned;

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
function solid(form: "ball" | "cube", centre: Vec3, axes: readonly [Vec3, Vec3, Vec3], approximate: boolean): Solid | undefined {
  // The matrix whose columns are the half-axes takes the unit solid onto this one.
  const toUnit = inverse(transpose(axes));
  if (toUnit === undefined) return undefined;
  const reach = [0, 1, 2].map((i) =>
    form === "cube" ? axes.reduce((s, a) => s + Math.abs(a[i] ?? 0), 0) : Math.sqrt(axes.reduce((s, a) => s + (a[i] ?? 0) ** 2, 0)),
  ) as unknown as Vec3;
  return { kind: "solid", form, centre, toUnit, approximate, bounds: { min: sub(centre, reach), max: add(centre, reach) } };
}

/** The box bounding `points`; none for no points. */
function boxOf(points: readonly Vec3[]): Box | undefined {
  if (points.length === 0) return undefined;
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (const p of points) {
    for (let i = 0; i < 3; i += 1) {
      min[i] = Math.min(min[i] ?? 0, p[i] ?? 0);
      max[i] = Math.max(max[i] ?? 0, p[i] ?? 0);
    }
  }
  return { min: min as unknown as Vec3, max: max as unknown as Vec3 };
}

const aligned = (bounds: Box): Aligned => ({ kind: "box", approximate: true, bounds });

/** The structure's bounding box, in its centre's unit and the ICRS; none where it has none. */
function boundsBox(detail: StructureDetail): Aligned | undefined {
  const k = detail.centre === null ? undefined : toMpc(detail.centre.unit);
  if (detail.bounds === null || k === undefined) return undefined;
  return aligned({ min: scale(detail.bounds.min, k), max: scale(detail.bounds.max, k) });
}

/** The angle across a cell of `order`, radians, with room to spare: the cells' corners at the
 * near and far distances bound the region but for its edges' bulge, which this covers. */
const cellAngle = (order: number) => 2 * Math.sqrt((4 * Math.PI) / (12 * 4 ** order));

/** The box bounding the sky region whose cells' centres, in the ICRS, are `centres`. */
function skyBounds(centres: readonly Vec3[], order: number, near: number, far: number): Box | undefined {
  const box = boxOf(centres.flatMap((c) => [scale(c, near), scale(c, far)]));
  if (box === undefined) return undefined;
  const pad = far * cellAngle(order);
  return { min: sub(box.min, [pad, pad, pad]), max: add(box.max, [pad, pad, pad]) };
}

/** Where a shape's frame and unit place it: a position, and a direction, in ICRS megaparsecs. */
interface Placing {
  place: (v: Vec3) => Vec3;
  turn: (v: Vec3) => Vec3;
  toFrame: Mat3;
  k: number;
}

type Geometry = NonNullable<StructureDetail["shape"]>["geometry"];
type Primitive = Extract<Geometry, { kind: "primitive" }>["primitive"];
type SkyRegion = Extract<Geometry, { kind: "sky_region" }>;

/** The solid of a sphere, an ellipsoid or an oriented box. */
function primitiveVolume(p: Primitive, { place, turn, k }: Placing): Volume | undefined {
  const centre = place(p.centre);
  if (p.form === "sphere") {
    const r = p.radius * k;
    return solid("ball", centre, [turn([r, 0, 0]), turn([0, r, 0]), turn([0, 0, r])], false);
  }
  // The columns of `orientation` are the solid's axes in the frame.
  const [u, v, w] = transpose(p.orientation);
  const [a, b, c] = p.form === "ellipsoid" ? p.semi_axes : p.half_extents;
  return solid(p.form === "ellipsoid" ? "ball" : "cube", centre, [place(scale(u, a)), place(scale(v, b)), place(scale(w, c))], false);
}

/** A sky region's cells between its near and far distances; none where it has no cells. */
function skyVolume(region: SkyRegion, { turn, toFrame, k }: Placing): Volume | undefined {
  const near = region.near * k;
  const far = region.far * k;
  const bounds = skyBounds(
    region.cells.map((cell) => turn(centreOf(region.order, cell))),
    region.order,
    near,
    far,
  );
  return bounds === undefined ? undefined : { kind: "sky", order: region.order, cells: new Set(region.cells), toFrame, near, far, approximate: false, bounds };
}

/** The box a point's published extents span about it, approximate; none without extents. */
function extentsVolume(at: Vec3, detail: StructureDetail, { place }: Placing): Volume | undefined {
  const axes = detail.derivation?.extents?.axes;
  if (axes === undefined) return undefined;
  const [x, y, z] = axes;
  return solid("cube", place(at), [place(scale(x, 0.5)), place(scale(y, 0.5)), place(scale(z, 0.5))], true);
}

/** How a shape in `frame` and `unit` is placed in the ICRS; none where the explorer cannot. */
function placingOf(frame: string, unit: string, rotations: Rotations): Placing | undefined {
  const k = toMpc(unit);
  const toFrame = toFrameOf(frame, rotations);
  if (k === undefined || toFrame === undefined) return undefined;
  const turn = (v: Vec3) => unapply(toFrame, v);
  return { place: (v) => turn(scale(v, k)), turn, toFrame, k };
}

/** The volume of `geometry`, placed by `placing`; the structure's bounding box for a grid, and
 * none for a path, a ray or a range. */
function geometryVolume(geometry: Geometry, detail: StructureDetail, placing: Placing): Volume | undefined {
  switch (geometry.kind) {
    case "primitive":
      return primitiveVolume(geometry.primitive, placing);
    case "sky_region":
      return geometry.order > MAX_CELL_ORDER ? boundsBox(detail) : skyVolume(geometry, placing);
    case "members": {
      const box = boxOf(geometry.members.map((m) => placing.place(m.at)));
      return box === undefined ? undefined : aligned(box);
    }
    case "point":
      return extentsVolume(geometry.at, detail, placing);
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
  const placing = shape === null ? undefined : placingOf(shape.frame, shape.unit, rotations);
  return shape === null || placing === undefined ? boundsBox(detail) : geometryVolume(shape.geometry, detail, placing);
}

/** Whether the point at `x`, `y`, `z`, in ICRS megaparsecs, lies inside `volume`. */
export function inside(volume: Volume, x: number, y: number, z: number): boolean {
  const { min, max } = volume.bounds;
  if (x < min[0] || x > max[0] || y < min[1] || y > max[1] || z < min[2] || z > max[2]) return false;
  switch (volume.kind) {
    case "box":
      return true;
    case "solid": {
      const [r0, r1, r2] = volume.toUnit;
      const [dx, dy, dz] = [x - volume.centre[0], y - volume.centre[1], z - volume.centre[2]];
      const u = r0[0] * dx + r0[1] * dy + r0[2] * dz;
      const v = r1[0] * dx + r1[1] * dy + r1[2] * dz;
      const w = r2[0] * dx + r2[1] * dy + r2[2] * dz;
      return volume.form === "ball" ? u * u + v * v + w * w <= 1 : Math.abs(u) <= 1 && Math.abs(v) <= 1 && Math.abs(w) <= 1;
    }
    case "sky": {
      const r = Math.hypot(x, y, z);
      return r >= volume.near && r <= volume.far && volume.cells.has(cellOf(volume.order, apply(volume.toFrame, [x, y, z])));
    }
  }
}

/** Which of the points at `offsets` from `centre` lie inside `volume`: 1 inside, 0 outside. */
export function insideOf(volume: Volume, centre: Vec3, offsets: Float32Array): Uint8Array {
  const mask = new Uint8Array(offsets.length / 3);
  const [cx, cy, cz] = centre;
  for (let i = 0; i < mask.length; i += 1) {
    mask[i] = inside(volume, cx + (offsets[3 * i] ?? 0), cy + (offsets[3 * i + 1] ?? 0), cz + (offsets[3 * i + 2] ?? 0)) ? 1 : 0;
  }
  return mask;
}

/** Whether the cube about `centre` with edges `edge` long meets the box `box`. */
export function meets(box: Box, centre: Vec3, edge: number): boolean {
  const h = edge / 2;
  for (let i = 0; i < 3; i += 1) {
    if ((centre[i] ?? 0) + h < (box.min[i] ?? 0) || (centre[i] ?? 0) - h > (box.max[i] ?? 0)) return false;
  }
  return true;
}
