// A structure as the view draws it (see-the-structures): its placed shape as outlines and
// markers in ICRS megaparsecs, a sky region as the boundary of its cells at its near and far
// distances, or its bounding box, marked approximate, where its shape cannot be drawn as
// itself; nothing where it has neither.

import type { Rotations } from "../api/schemas.ts";
import type { Geometry, StructureDetail } from "../api/structures.ts";
import { frameOf } from "./frames.ts";
import { boundary } from "./healpix.ts";
import { type Outline, boundsEdges, boxEdges, empty, lines, polyline, rings } from "./outline.ts";
import { toMpc } from "./units.ts";
import { type Mat3, type Vec3, add, cross, dot, norm, normalize, scale, sub, transpose, unapply } from "./vec.ts";

/** How a structure is drawn. */
export interface StructureDrawing {
  id: string;
  name: string;
  /** Where its name is drawn; none when it is not drawn. */
  centre: Vec3 | undefined;
  /** The diagonal of its bounding box, megaparsecs, where it has one. */
  size: number | undefined;
  outline: Outline;
  /** Drawn as its bounding box rather than its shape. */
  approximate: boolean;
}

/** The rotation from the ICRS to a shape's `frame`, if the explorer can place it. */
function toFrameOf(frame: string, rotations: Rotations): Mat3 | undefined {
  const axes = frame === "icrs" ? "equatorial" : frame;
  return axes === "galactic" || axes === "supergalactic" || axes === "equatorial" ? frameOf(axes, rotations)?.toFrame : undefined;
}

/** A position in the shape's frame and unit, in ICRS megaparsecs. */
type Place = (v: Vec3) => Vec3;

/** A direction in the shape's frame, in the ICRS. */
type Turn = (v: Vec3) => Vec3;

type Primitive = Extract<Geometry, { kind: "primitive" }>["primitive"];

function primitiveOutline(primitive: Primitive, place: Place, turn: Turn, k: number): Outline {
  const centre = place(primitive.centre);
  if (primitive.form === "sphere") {
    // Rings in the planes of the shape's own frame.
    const r = primitive.radius * k;
    return lines(rings(centre, [turn([r, 0, 0]), turn([0, r, 0]), turn([0, 0, r])]));
  }
  // The columns of `orientation` are the solid's axes in the frame.
  const [u, v, w] = transpose(primitive.orientation);
  const [a, b, c] = primitive.form === "ellipsoid" ? primitive.semi_axes : primitive.half_extents;
  const axes: [Vec3, Vec3, Vec3] = [turn(scale(u, a * k)), turn(scale(v, b * k)), turn(scale(w, c * k))];
  return lines(primitive.form === "ellipsoid" ? rings(centre, axes) : boxEdges(centre, axes));
}

type SkyRegion = Extract<Geometry, { kind: "sky_region" }>;

/** The corners where a region's radial edges run: the boundary's points furthest each way along
 * two axes across `towards`, the region's mean direction. */
function extremes(points: readonly Vec3[], towards: Vec3): Vec3[] {
  const across = normalize(cross(towards, Math.abs(towards[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0]));
  const up = cross(towards, across);
  const furthest = (axis: Vec3, sign: number) => {
    let best = points[0] ?? towards;
    let most = -Infinity;
    for (const p of points) {
      const along = sign * dot(p, axis);
      if (along > most) [best, most] = [p, along];
    }
    return best;
  };
  return [furthest(across, 1), furthest(across, -1), furthest(up, 1), furthest(up, -1)];
}

/** The finest order whose cells the explorer outlines: finer cells' numbers and corners are
 * beyond its arithmetic. Univ Ingest makes regions of order 6. */
const FINEST_ORDER = 20;

/** How far the boundary's mean direction must reach, of its points', for the region to have
 * extremes: a band about the sky has none. */
const LEANING = 0.05;

/** A sky region: the boundary of its cells on the spheres of its near and far distances, joined
 * by radial edges at its extremes. Its cells' directions lie in the shape's frame. None where it
 * has no boundary, the whole sky, or its cells are too fine: it is drawn by its box. */
function skyRegionOutline(region: SkyRegion, turn: Turn, k: number): Outline | undefined {
  if (region.order > FINEST_ORDER) return undefined;
  const edges = boundary(region.order, region.cells).map((line) => line.map(turn));
  if (edges.length === 0) return undefined;
  const near = region.near * k;
  const far = region.far * k;
  // The boundary's mean direction is enough to set the axes across the region.
  const points = edges.flat();
  const mean = points.reduce<Vec3>((sum, p) => add(sum, p), [0, 0, 0]);
  const leaning = norm(mean) / points.length >= LEANING;
  const radial = leaning ? extremes(points, normalize(mean)).flatMap((p) => [scale(p, near), scale(p, far)]) : [];
  const shells = [near, far].flatMap((r) => edges.flatMap((line) => polyline(line.map((p) => scale(p, r)))));
  return lines([...shells, ...radial]);
}

/** The outline of `geometry`; none for a grid, which is drawn by its box. */
function outlineOf(geometry: Geometry, place: Place, turn: Turn, k: number): Outline | undefined {
  switch (geometry.kind) {
    case "point":
      return { segments: [], markers: [place(geometry.at)] };
    case "ray":
      return lines([place(geometry.origin), place(add(geometry.origin, scale(geometry.direction, geometry.reach)))]);
    case "range":
      return lines([place(scale(geometry.direction, geometry.near)), place(scale(geometry.direction, geometry.far))]);
    case "primitive":
      return primitiveOutline(geometry.primitive, place, turn, k);
    case "path":
      return lines(polyline(geometry.points.map(place)));
    case "members":
      return { segments: [], markers: geometry.members.map((m) => place(m.at)) };
    case "sky_region":
      return skyRegionOutline(geometry, turn, k);
    default:
      return undefined;
  }
}

/** One line along each of a point's published extents, centred on the point. */
function extentLines(at: Vec3, axes: readonly Vec3[], place: Place): Vec3[] {
  return axes.flatMap((axis) => [place(sub(at, scale(axis, 0.5))), place(add(at, scale(axis, 0.5)))]);
}

/** The shape's outline in ICRS megaparsecs; none where its frame, unit or kind can't be drawn.
 * A point with published extents has a line through it along each. */
function shapeOutline(detail: StructureDetail, rotations: Rotations): Outline | undefined {
  const shape = detail.shape;
  if (shape === null) return undefined;
  const k = toMpc(shape.unit);
  const toFrame = toFrameOf(shape.frame, rotations);
  if (k === undefined || toFrame === undefined) return undefined;
  const turn: Turn = (v) => unapply(toFrame, v);
  const place: Place = (v) => turn(scale(v, k));
  const outline = outlineOf(shape.geometry, place, turn, k);
  const extents = detail.derivation?.extents;
  if (outline === undefined || shape.geometry.kind !== "point" || extents === undefined) return outline;
  return { ...outline, segments: extentLines(shape.geometry.at, extents.axes, place) };
}

/** The structure's centre and bounds in ICRS megaparsecs: both are given in the centre's unit
 * (and its frame, the ICRS). */
function placed(detail: StructureDetail): { centre: Vec3 | undefined; min: Vec3; max: Vec3 } | undefined {
  const c = detail.centre;
  const k = c === null ? undefined : toMpc(c.unit);
  if (c === null || k === undefined) return undefined;
  const centre = scale([c.x, c.y, c.z], k);
  if (detail.bounds === null) return { centre, min: centre, max: centre };
  return { centre, min: scale(detail.bounds.min, k), max: scale(detail.bounds.max, k) };
}

/** The diagonal of the box a point's published extents span, megaparsecs: its size, since its
 * bounds are only its point. */
function extentsSize(detail: StructureDetail): number | undefined {
  const axes = detail.derivation?.extents?.axes;
  const k = detail.shape === null ? undefined : toMpc(detail.shape.unit);
  return axes === undefined || k === undefined ? undefined : norm(axes.reduce<Vec3>((sum, a) => add(sum, scale(a, k)), [0, 0, 0]));
}

/** The structure's name, with "(at least)" where its papers give its shape as a lower limit on
 * its extent. */
export function structureName(detail: StructureDetail): string {
  return detail.derivation?.at_least === undefined ? detail.name : `${detail.name} (at least)`;
}

/** How `detail` is drawn, its shape turned into the ICRS by `rotations`. */
export function drawingOf(detail: StructureDetail, rotations: Rotations): StructureDrawing {
  const base = { id: detail.id, name: structureName(detail) };
  const where = placed(detail);
  const size = extentsSize(detail) ?? (where === undefined || detail.bounds === null ? undefined : norm(sub(where.max, where.min)));
  const outline = shapeOutline(detail, rotations);
  if (outline !== undefined) return { ...base, centre: where?.centre, size, outline, approximate: false };
  if (where === undefined || detail.bounds === null) return { ...base, centre: undefined, size: undefined, outline: empty(), approximate: false };
  return { ...base, centre: where.centre, size, outline: lines(boundsEdges(where.min, where.max)), approximate: true };
}
