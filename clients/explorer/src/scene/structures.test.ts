import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { manifestOf } from "../test/fixtures.ts";
import { shapeOf, structureDetail } from "../test/structures.ts";
import { drawingOf } from "./structures.ts";
import { type Vec3, norm, normalize, sub, unapply } from "./vec.ts";

const ROTATIONS = manifestOf("stars").rotations;

function detail(shape: StructureDetail["shape"], bounds: StructureDetail["bounds"] = { min: [-1, -1, -1], max: [1, 1, 1] }): StructureDetail {
  return structureDetail({ shape, bounds, centre: bounds === null ? null : { x: 0, y: 0, z: 0, unit: "megaparsec" } });
}

const distances = (from: Vec3, points: Vec3[]) => points.map((p) => norm(sub(p, from)));

describe("a structure's name", () => {
  it("is followed by \"(at least)\" where its papers give its shape as a lower limit on its extent", () => {
    const point = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [1, 0, 0] } });
    const plain = { ...detail(point), name: "Vela Supercluster" };
    expect(drawingOf(plain, ROTATIONS).name).toBe("Vela Supercluster");
    const atLeast = { ...plain, derivation: { from: [], at_least: "/geometry/SkyRegion/at_least" } };
    expect(drawingOf(atLeast, ROTATIONS).name).toBe("Vela Supercluster (at least)");
  });
});

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

  it("draws a point's published extents as a line along each, centred on its marker, and no box", () => {
    const point = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [10, 0, 0] } });
    const axes: [[number, number, number], [number, number, number], [number, number, number]] = [[4, 0, 0], [0, 6, 0], [0, 0, 8]];
    const drawing = drawingOf({ ...detail(point), derivation: { from: [], extents: { frame: "supergalactic", axes } } }, ROTATIONS);
    expect(drawing.approximate).toBe(false);
    expect(drawing.outline.markers).toEqual([[10, 0, 0]]);
    expect(drawing.outline.segments).toEqual([[8, 0, 0], [12, 0, 0], [10, -3, 0], [10, 3, 0], [10, 0, -4], [10, 0, 4]]);
    // Its size, which frames it when chosen, is the diagonal its extents span.
    expect(drawing.size).toBeCloseTo(Math.hypot(4, 6, 8));
    expect(drawingOf(detail(point), ROTATIONS).outline.segments).toEqual([]);
  });

  it("draws a grid, or a structure with no placed shape, as its bounding box, marked approximate", () => {
    for (const shape of [shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "volume_grid" } }), null]) {
      const drawing = drawingOf(detail(shape), ROTATIONS);
      expect(drawing.approximate).toBe(true);
      expect(drawing.outline.segments.length).toBe(24);
      expect(drawing.centre).toEqual([0, 0, 0]);
    }
  });

  it("draws a sky region as the boundary of its cells at its near and far distances, joined by radial edges", () => {
    // HEALPix order 1, cell 0: one cell, its four edges at 10 and at 20 Mpc, and four radial edges.
    const region = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "sky_region", order: 1, cells: [0], near: 10, far: 20 } });
    const drawing = drawingOf(detail(region), ROTATIONS);
    expect(drawing.approximate).toBe(false);
    const segments = drawing.outline.segments;
    // Two shells of four edges, each in three pieces of two points, and four radial edges.
    expect(segments.length).toBe(2 * 4 * 3 * 2 + 4 * 2);
    const reach = segments.map((p) => norm(p));
    for (const r of reach.slice(0, 24)) expect(r).toBeCloseTo(10, 9);
    for (const r of reach.slice(24, 48)) expect(r).toBeCloseTo(20, 9);
    for (const [i, r] of reach.slice(48).entries()) expect(r).toBeCloseTo(i % 2 === 0 ? 10 : 20, 9);
  });

  it("draws by its box a sky region with no boundary to draw, and gives a band about the sky no radial edges", () => {
    const sky = (order: number, cells: number[]) => shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "sky_region", order, cells, near: 1, far: 2 } });
    // The whole sky has no boundary; a region of order 21 is finer than the explorer outlines.
    for (const shape of [sky(0, Array.from({ length: 12 }, (_, i) => i)), sky(21, [0])]) {
      expect(drawingOf(detail(shape), ROTATIONS).approximate).toBe(true);
    }
    // The four equatorial base cells: a band about the equator, its mean direction nowhere.
    // They meet only at corners: sixteen edges, in three pieces each, on two shells, and no more.
    expect(drawingOf(detail(sky(0, [4, 5, 6, 7])), ROTATIONS).outline.segments).toHaveLength(16 * 3 * 2 * 2);
  });

  it("turns a sky region's cells from the frame its shape names", () => {
    const geometry = { kind: "sky_region" as const, order: 0, cells: [4], near: 1, far: 2 };
    const icrs = drawingOf(detail(shapeOf({ frame: "icrs", unit: "megaparsec", geometry })), ROTATIONS).outline.segments;
    const galactic = drawingOf(detail(shapeOf({ frame: "galactic", unit: "megaparsec", geometry })), ROTATIONS).outline.segments;
    // Base cell 4 is centred on longitude 0 at the equator: the frame's x axis.
    const centre = (points: Vec3[]) => normalize(points.reduce<Vec3>((sum, p) => [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]], [0, 0, 0]));
    expect(centre(icrs)[0]).toBeCloseTo(1, 1);
    const towards = unapply(ROTATIONS.to_galactic, [1, 0, 0]);
    for (const [i, c] of centre(galactic).entries()) expect(c).toBeCloseTo(towards[i] ?? 0, 1);
  });

  it("takes the bounds and centre in the centre's unit", () => {
    const inParsecs = structureDetail({ centre: { x: 0, y: 0, z: 0, unit: "parsec" }, bounds: { min: [-50, -50, -50], max: [50, 50, 50] } });
    const drawing = drawingOf(inParsecs, ROTATIONS);
    expect(Math.max(...drawing.outline.segments.map((p) => Math.abs(p[0])))).toBeCloseTo(50e-6);
    expect(drawing.size).toBeCloseTo(Math.hypot(100, 100, 100) * 1e-6);
  });

  it("lists but does not draw a structure with neither a shape nor a bounding box", () => {
    const drawing = drawingOf(detail(null, null), ROTATIONS);
    expect(drawing.centre).toBeUndefined();
    expect(drawing.outline).toEqual({ segments: [], markers: [] });
  });
});
