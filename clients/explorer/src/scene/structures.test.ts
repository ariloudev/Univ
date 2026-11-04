import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { manifestOf } from "../test/fixtures.ts";
import { drawingOf } from "./structures.ts";
import { type Vec3, norm, sub, unapply } from "./vec.ts";

/** A placed shape, typed as Univ Server gives it. */
const shapeOf = (shape: NonNullable<StructureDetail["shape"]>) => shape;

const ROTATIONS = manifestOf("stars").rotations;

function detail(shape: StructureDetail["shape"], bounds: StructureDetail["bounds"] = { min: [-1, -1, -1], max: [1, 1, 1] }): StructureDetail {
  return {
    id: "s",
    name: "S",
    kind: "void",
    scale_domain: "cosmic_web",
    centre: bounds === null ? null : { x: 0, y: 0, z: 0, unit: "megaparsec" },
    bounds,
    shape,
    record: {},
  };
}

const distances = (from: Vec3, points: Vec3[]) => points.map((p) => norm(sub(p, from)));

describe("a structure's shape", () => {
  it("draws a sphere as rings at its radius, in the frame and unit its shape names", () => {
    const shape = shapeOf({ frame: "galactic", unit: "kiloparsec", geometry: { kind: "primitive", primitive: { form: "sphere", centre: [1000, 0, 0], radius: 500 } } });
    const drawing = drawingOf(detail(shape), ROTATIONS);
    expect(drawing.approximate).toBe(false);
    // Galactic (1 Mpc, 0, 0) is the Galactic centre's direction, in the ICRS.
    const centre = unapply(ROTATIONS.to_galactic, [1, 0, 0]);
    for (const d of distances(centre, drawing.outline.segments)) expect(d).toBeCloseTo(0.5, 9);
  });

  it("draws an ellipsoid as rings along its axes, and an oriented box as its twelve edges", () => {
    const orientation: [[number, number, number], [number, number, number], [number, number, number]] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
    const ellipsoid = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "ellipsoid", centre: [0, 0, 0], semi_axes: [3, 2, 1], orientation } } });
    const reach = distances([0, 0, 0], drawingOf(detail(ellipsoid), ROTATIONS).outline.segments);
    expect(Math.max(...reach)).toBeCloseTo(3);
    expect(Math.min(...reach)).toBeCloseTo(1);
    const box = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "oriented_box", centre: [0, 0, 0], half_extents: [3, 2, 1], orientation } } });
    const edges = drawingOf(detail(box), ROTATIONS).outline.segments;
    expect(edges.length).toBe(24);
    for (const corner of edges) expect(norm(corner)).toBeCloseTo(Math.hypot(3, 2, 1));
  });

  it("draws a path as lines, members as markers at their positions, and a point as a marker", () => {
    const path = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "path", points: [[0, 0, 0], [1, 0, 0], [1, 1, 0]] } });
    expect(drawingOf(detail(path), ROTATIONS).outline.segments).toEqual([[0, 0, 0], [1, 0, 0], [1, 0, 0], [1, 1, 0]]);
    const members = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "members", members: [{ at: [5, 0, 0] }, { at: [0, 5, 0] }] } });
    expect(drawingOf(detail(members), ROTATIONS).outline.markers).toEqual([[5, 0, 0], [0, 5, 0]]);
    const point = shapeOf({ frame: "icrs", unit: "parsec", geometry: { kind: "point", at: [2e6, 0, 0] } });
    expect(drawingOf(detail(point), ROTATIONS).outline.markers).toEqual([[2, 0, 0]]);
  });

  it("draws a sky region, or a structure with no placed shape, as its bounding box, marked approximate", () => {
    for (const shape of [shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "sky_region" } }), null]) {
      const drawing = drawingOf(detail(shape), ROTATIONS);
      expect(drawing.approximate).toBe(true);
      expect(drawing.outline.segments.length).toBe(24);
      expect(drawing.centre).toEqual([0, 0, 0]);
    }
  });

  it("lists but does not draw a structure with neither a shape nor a bounding box", () => {
    const drawing = drawingOf(detail(null, null), ROTATIONS);
    expect(drawing.centre).toBeUndefined();
    expect(drawing.outline).toEqual({ segments: [], markers: [] });
  });
});
