// A structure as the view draws it (see-the-structures): its placed shape as outlines and
// markers in ICRS megaparsecs, or its bounding box, marked approximate, where its shape cannot
// be drawn as itself; nothing where it has neither.

import type { Rotations } from "../api/schemas.ts";
import type { Geometry, StructureDetail } from "../api/structures.ts";
import { type Outline, boundsEdges, boxEdges, empty, polyline, rings } from "./outline.ts";
import { IDENTITY, type Mat3, type Vec3, add, scale, transpose, unapply } from "./vec.ts";

/** How a structure is drawn. */
export interface StructureDrawing {
  id: string;
  name: string;
  /** Where its name is drawn; none when it is not drawn. */
  centre: Vec3 | undefined;
  outline: Outline;
  /** Drawn as its bounding box rather than its shape. */
  approximate: boolean;
}

/** Megaparsecs in each length unit a shape may be in; `megaparsec_over_h` needs h, so is not. */
const MPC_PER: Record<string, number> = {
  megaparsec: 1,
  kiloparsec: 1e-3,
  parsec: 1e-6,
  // IAU 2012: 1 au = 149 597 870 700 m; 1 pc = 648 000/π au; 1 ly = 9 460 730 472 580 800 m.
  light_year: 9_460_730_472_580_800 / (149_597_870_700 * (648_000 / Math.PI)) / 1e6,
  astronomical_unit: Math.PI / 648_000 / 1e6,
};

/** The rotation from the ICRS to `frame`, if the explorer can place it. */
function toFrameOf(frame: string, rotations: Rotations): Mat3 | undefined {
  if (frame === "icrs") return IDENTITY;
  if (frame === "galactic") return rotations.to_galactic;
  if (frame === "supergalactic") return rotations.to_supergalactic;
  return undefined;
}

/** A position in the shape's frame and unit, in ICRS megaparsecs. */
type Place = (v: Vec3) => Vec3;

/** A direction in the shape's frame, in the ICRS. */
type Turn = (v: Vec3) => Vec3;

function primitiveOutline(primitive: Extract<Geometry, { kind: "primitive" }>["primitive"], place: Place, turn: Turn, k: number): Outline {
  const centre = place(primitive.centre);
  if (primitive.form === "sphere") {
    const r = primitive.radius * k;
    return { segments: rings(centre, [turn([r, 0, 0]), turn([0, r, 0]), turn([0, 0, r])]), markers: [] };
  }
  // The columns of `orientation` are the solid's axes in the frame.
  const columns = transpose(primitive.orientation);
  const sizes = primitive.form === "ellipsoid" ? primitive.semi_axes : primitive.half_extents;
  const axes = [0, 1, 2].map((i) => turn(scale(columns[i as 0 | 1 | 2], sizes[i as 0 | 1 | 2] * k))) as [Vec3, Vec3, Vec3];
  return { segments: primitive.form === "ellipsoid" ? rings(centre, axes) : boxEdges(centre, axes), markers: [] };
}

/** The outline of `geometry`; none for a sky region or a grid, which are drawn by their box. */
function outlineOf(geometry: Geometry, place: Place, turn: Turn, k: number): Outline | undefined {
  switch (geometry.kind) {
    case "point":
      return { segments: [], markers: [place(geometry.at)] };
    case "ray":
      return { segments: [place(geometry.origin), place(add(geometry.origin, scale(geometry.direction, geometry.reach)))], markers: [] };
    case "range":
      return { segments: [place(scale(geometry.direction, geometry.near)), place(scale(geometry.direction, geometry.far))], markers: [] };
    case "primitive":
      return primitiveOutline(geometry.primitive, place, turn, k);
    case "path":
      return { segments: polyline(geometry.points.map(place)), markers: [] };
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
  const k = MPC_PER[shape.unit];
  const toFrame = toFrameOf(shape.frame, rotations);
  if (k === undefined || toFrame === undefined) return undefined;
  const turn: Turn = (v) => unapply(toFrame, v);
  return outlineOf(shape.geometry, (v) => turn(scale(v, k)), turn, k);
}

/** How `detail` is drawn, its shape turned into the ICRS by `rotations`. */
export function drawingOf(detail: StructureDetail, rotations: Rotations): StructureDrawing {
  const c = detail.centre;
  const centre = c === null ? undefined : scale([c.x, c.y, c.z], MPC_PER[c.unit] ?? 1);
  const base = { id: detail.id, name: detail.name };
  const outline = shapeOutline(detail, rotations);
  if (outline !== undefined) return { ...base, centre, outline, approximate: false };
  if (detail.bounds === null) return { ...base, centre: undefined, outline: empty(), approximate: false };
  const { min, max } = detail.bounds;
  return { ...base, centre: centre ?? scale(add(min, max), 0.5), outline: { segments: boundsEdges(min, max), markers: [] }, approximate: true };
}
