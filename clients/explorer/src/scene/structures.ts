// A structure as the view draws it (see-the-structures): its placed shape as outlines and
// markers in ICRS megaparsecs, a sky region as the boundary of its cells at its near and far
// distances, or its bounding box, marked approximate, where its shape cannot be drawn as
// itself; nothing where it has neither.

import type { Rotations } from "../api/schemas.ts";
import type { Geometry, StructureDetail } from "../api/structures.ts";
import { frameOf } from "./frames.ts";
import { boundary } from "./healpix.ts";
import { FLOWS } from "./kinds.ts";
import { type Outline, boundsEdges, boxEdges, empty, flowSymbol, lines, polyline, rings } from "./outline.ts";
import { toMpc } from "./units.ts";
import { type Mat3, type Vec3, add, cross, dot, norm, normalize, scale, sub, transpose, unapply } from "./vec.ts";

/** How a structure is drawn. */
export interface StructureDrawing {
  id: string;
  name: string;
  /** Its kind, whose colour it is drawn in. */
  kind: string;
  /** Where its name is drawn; none when it is not drawn. */
  centre: Vec3 | undefined;
  /** The diagonal of its bounding box, megaparsecs, where it has one. */
  size: number | undefined;
  outline: Outline;
  /** Drawn as its bounding box rather than its shape. */
  approximate: boolean;
  /** The symbol of an attractor or a repeller, about its centre: segments in radii of a sphere
   * drawn at a fixed size on the view. None for any other kind, or one with no centre. */
  symbol: Vec3[] | undefined;
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

/** How a shape is placed in ICRS megaparsecs: a position, a direction, the rotation from the
 * ICRS to its frame, and the megaparsecs in its unit. The view's outline and the cull to its
 * volume both place a shape by it. */
export interface Placing {
  place: Place;
  turn: Turn;
  toFrame: Mat3;
  k: number;
}

/** How a shape in `frame` and `unit` is placed; none where the explorer cannot place it. */
export function placingOf(shape: { frame: string; unit: string }, rotations: Rotations): Placing | undefined {
  const k = toMpc(shape.unit);
  const toFrame = toFrameOf(shape.frame, rotations);
  if (k === undefined || toFrame === undefined) return undefined;
  const turn: Turn = (v) => unapply(toFrame, v);
  return { place: (v) => turn(scale(v, k)), turn, toFrame, k };
}

export type Primitive = Extract<Geometry, { kind: "primitive" }>["primitive"];

/** A sphere, an ellipsoid or an oriented box as its centre and half-axes in ICRS megaparsecs,
 * round or not. A sphere's axes are those of its shape's frame. */
export function primitiveAxes(primitive: Primitive, { place, turn, k }: Placing): { round: boolean; centre: Vec3; axes: [Vec3, Vec3, Vec3] } {
  const centre = place(primitive.centre);
  if (primitive.form === "sphere") {
    const r = primitive.radius * k;
    return { round: true, centre, axes: [turn([r, 0, 0]), turn([0, r, 0]), turn([0, 0, r])] };
  }
  // The columns of `orientation` are the solid's axes in the frame.
  const [u, v, w] = transpose(primitive.orientation);
  const [a, b, c] = primitive.form === "ellipsoid" ? primitive.semi_axes : primitive.half_extents;
  return { round: primitive.form === "ellipsoid", centre, axes: [place(scale(u, a)), place(scale(v, b)), place(scale(w, c))] };
}

function primitiveOutline(primitive: Primitive, placing: Placing): Outline {
  const { round, centre, axes } = primitiveAxes(primitive, placing);
  return lines(round ? rings(centre, axes) : boxEdges(centre, axes));
}

export type SkyRegion = Extract<Geometry, { kind: "sky_region" }>;

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

/** The finest order whose cells the explorer outlines and culls to: finer cells' numbers and
 * corners are beyond its arithmetic. Univ Ingest makes regions of order 6. */
export const FINEST_ORDER = 20;

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
function outlineOf(geometry: Geometry, placing: Placing): Outline | undefined {
  const { place, turn, k } = placing;
  switch (geometry.kind) {
    case "point":
      return { segments: [], markers: [place(geometry.at)] };
    case "ray":
      return lines([place(geometry.origin), place(add(geometry.origin, scale(geometry.direction, geometry.reach)))]);
    case "range":
      return lines([place(scale(geometry.direction, geometry.near)), place(scale(geometry.direction, geometry.far))]);
    case "primitive":
      return primitiveOutline(geometry.primitive, placing);
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

/** Half of each of a point's published extents, in ICRS megaparsecs: they are whole lengths
 * centred on it, already ICRS vectors in the shape's unit, which has `k` megaparsecs, so they
 * are scaled and not turned. */
export const halfExtents = (axes: readonly [Vec3, Vec3, Vec3], k: number): [Vec3, Vec3, Vec3] => [
  scale(axes[0], 0.5 * k),
  scale(axes[1], 0.5 * k),
  scale(axes[2], 0.5 * k),
];

/** One line along each of a point's published extents, centred on the point. */
function extentLines(at: Vec3, axes: readonly [Vec3, Vec3, Vec3], { place, k }: Placing): Vec3[] {
  const centre = place(at);
  return halfExtents(axes, k).flatMap((half) => [sub(centre, half), add(centre, half)]);
}

/** The shape's outline in ICRS megaparsecs; none where its frame, unit or kind can't be drawn.
 * A point with published extents has a line through it along each. */
function shapeOutline(detail: StructureDetail, placing: Placing | undefined): Outline | undefined {
  const shape = detail.shape;
  if (shape === null || placing === undefined) return undefined;
  const outline = outlineOf(shape.geometry, placing);
  const extents = detail.derivation?.extents;
  if (outline === undefined || shape.geometry.kind !== "point" || extents === undefined) return outline;
  return { ...outline, segments: extentLines(shape.geometry.at, extents.axes, placing) };
}

/** The structure's centre and bounds in ICRS megaparsecs: both are given in the centre's unit
 * (and its frame, the ICRS). */
export function placed(detail: StructureDetail): { centre: Vec3 | undefined; min: Vec3; max: Vec3 } | undefined {
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

/** The flow symbol of a structure of `kind` if it is an attractor or a repeller, its arrows along
 * the axes of its shape's frame, `placing`, or of the ICRS where it has no shape the explorer can
 * place. */
function symbolOf(kind: string, placing: Placing | undefined): Vec3[] | undefined {
  const flow = FLOWS[kind];
  if (flow === undefined) return undefined;
  const turn: Turn = placing?.turn ?? ((v) => v);
  return flowSymbol(flow, [turn([1, 0, 0]), turn([0, 1, 0]), turn([0, 0, 1])]);
}

/** How `detail` is drawn, its shape turned into the ICRS by `rotations`. */
export function drawingOf(detail: StructureDetail, rotations: Rotations): StructureDrawing {
  const where = placed(detail);
  const placing = detail.shape === null ? undefined : placingOf(detail.shape, rotations);
  const symbol = where?.centre === undefined ? undefined : symbolOf(detail.kind, placing);
  const base = { id: detail.id, name: structureName(detail), kind: detail.kind, symbol };
  const size = extentsSize(detail) ?? (where === undefined || detail.bounds === null ? undefined : norm(sub(where.max, where.min)));
  const outline = shapeOutline(detail, placing);
  if (outline !== undefined) return { ...base, centre: where?.centre, size, outline, approximate: false };
  if (where === undefined || detail.bounds === null) return { ...base, centre: undefined, size: undefined, outline: empty(), approximate: false };
  return { ...base, centre: where.centre, size, outline: lines(boundsEdges(where.min, where.max)), approximate: true };
}

/** Whether the outline, markers and flow symbol named `name` are drawn (see-the-structures):
 * never where its kind is turned off, and, while the outlines are off, only where it is chosen. */
export const outlineDrawn = (name: string, { hidden, chosen, outlines }: { hidden: ReadonlySet<string>; chosen: string | undefined; outlines: boolean }): boolean =>
  !hidden.has(name) && (outlines || name === chosen);
