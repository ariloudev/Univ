// A structure as the view draws it (see-the-structures): its placed shape as outlines and
// markers in ICRS megaparsecs, or its bounding box, marked approximate, where its shape cannot
// be drawn as itself; nothing where it has neither.

import type { Rotations } from "../api/schemas.ts";
import type { Geometry, StructureDetail } from "../api/structures.ts";
import { frameOf } from "./frames.ts";
import { type Outline, boundsEdges, boxEdges, empty, lines, polyline, rings } from "./outline.ts";
import { toMpc } from "./units.ts";
import { type Mat3, type Vec3, add, norm, scale, sub, transpose, unapply } from "./vec.ts";

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

/** The outline of `geometry`; none for a sky region or a grid, which are drawn by their box. */
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
    default:
      return undefined;
  }
}

/** The shape's outline in ICRS megaparsecs; none where its frame, unit or kind can't be drawn. */
function shapeOutline(detail: StructureDetail, rotations: Rotations): Outline | undefined {
  const shape = detail.shape;
  if (shape === null) return undefined;
  const k = toMpc(shape.unit);
  const toFrame = toFrameOf(shape.frame, rotations);
  if (k === undefined || toFrame === undefined) return undefined;
  const turn: Turn = (v) => unapply(toFrame, v);
  return outlineOf(shape.geometry, (v) => turn(scale(v, k)), turn, k);
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

/** How `detail` is drawn, its shape turned into the ICRS by `rotations`. */
export function drawingOf(detail: StructureDetail, rotations: Rotations): StructureDrawing {
  const base = { id: detail.id, name: detail.name };
  const where = placed(detail);
  const size = where === undefined || detail.bounds === null ? undefined : norm(sub(where.max, where.min));
  const outline = shapeOutline(detail, rotations);
  if (outline !== undefined) return { ...base, centre: where?.centre, size, outline, approximate: false };
  if (where === undefined || detail.bounds === null) return { ...base, centre: undefined, size: undefined, outline: empty(), approximate: false };
  return { ...base, centre: where.centre, size, outline: lines(boundsEdges(where.min, where.max)), approximate: true };
}
