import { describe, expect, it } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { manifestOf } from "../test/fixtures.ts";
import { shapeOf, structureDetail } from "../test/structures.ts";
import { frameOf } from "./frames.ts";
import { cellOf, centreOf } from "./healpix.ts";
import { type Vec3, scale, unapply } from "./vec.ts";
import { type Volume, holds, inside, insideOf, meets, volumeOf } from "./volume.ts";

const ROTATIONS = manifestOf("stars").rotations;

function detail(shape: StructureDetail["shape"], overrides: Partial<StructureDetail> = {}): StructureDetail {
  return structureDetail({ shape, ...overrides });
}

function volume(shape: StructureDetail["shape"], overrides: Partial<StructureDetail> = {}): Volume {
  const found = volumeOf(detail(shape, overrides), ROTATIONS);
  if (found === undefined) throw new Error("no volume");
  return found;
}

const isIn = (v: Volume, p: Vec3) => inside(v, p[0], p[1], p[2]);

const S = Math.SQRT1_2;

describe("the volume a structure is culled to", () => {
  it("is the solid of a sphere, in the frame and unit its shape names", () => {
    const galactic = frameOf("galactic", ROTATIONS)?.toFrame;
    if (galactic === undefined) throw new Error("no galactic frame");
    const icrs = (v: Vec3) => unapply(galactic, scale(v, 1e-3));
    const v = volume(shapeOf({ frame: "galactic", unit: "kiloparsec", geometry: { kind: "primitive", primitive: { form: "sphere", centre: [10_000, 0, 0], radius: 2000 } } }));
    expect(v.approximate).toBe(false);
    expect(isIn(v, icrs([10_000, 0, 0]))).toBe(true);
    expect(isIn(v, icrs([11_900, 0, 0]))).toBe(true);
    expect(isIn(v, icrs([10_000, 1900, 0]))).toBe(true);
    expect(isIn(v, icrs([12_100, 0, 0]))).toBe(false);
    // Along the ICRS x axis, which is not galactic x, 11.9 Mpc out lies well outside.
    expect(isIn(v, [11.9, 0, 0])).toBe(false);
  });

  it("is the solid of an ellipsoid along its own axes", () => {
    const orientation: [[number, number, number], [number, number, number], [number, number, number]] = [
      [S, -S, 0],
      [S, S, 0],
      [0, 0, 1],
    ];
    const v = volume(
      shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "ellipsoid", centre: [0, 0, 0], semi_axes: [3, 1, 1], orientation } } }),
    );
    // Its first axis, the first column of its orientation, runs along x = y.
    expect(isIn(v, [2.9 * S, 2.9 * S, 0])).toBe(true);
    expect(isIn(v, [2 * S, 2 * S, 0.5])).toBe(true);
    expect(isIn(v, [2.9, 0, 0])).toBe(false);
    expect(isIn(v, [0, 0, 1.1])).toBe(false);
  });

  it("is the solid of an oriented box, to its corners", () => {
    const orientation: [[number, number, number], [number, number, number], [number, number, number]] = [
      [S, -S, 0],
      [S, S, 0],
      [0, 0, 1],
    ];
    const v = volume(
      shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "oriented_box", centre: [5, 0, 0], half_extents: [2, 1, 1], orientation } } }),
    );
    expect(isIn(v, [5 + 1.9 * S, 1.9 * S, 0])).toBe(true);
    // Just inside its corner: 2 along its first axis, (S, S, 0), and 1 along its second,
    // (-S, S, 0), and its third.
    expect(isIn(v, [5 + 0.99 * (2 * S - S), 0.99 * (2 * S + S), 0.99])).toBe(true);
    expect(isIn(v, [5 + 1.01 * (2 * S - S), 1.01 * (2 * S + S), 0.99])).toBe(false);
    expect(isIn(v, [5 + 1.9, 0, 0])).toBe(false);
  });

  it("is a sky region's cells between its near and far distances", () => {
    const cells = [cellOf(6, [1, 0, 0]), cellOf(6, [0, 1, 0])];
    const v = volume(shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "sky_region", order: 6, cells, near: 10, far: 20 } }));
    expect(v.approximate).toBe(false);
    expect(isIn(v, scale(centreOf(6, cells[0] ?? 0), 15))).toBe(true);
    expect(isIn(v, scale(centreOf(6, cells[1] ?? 0), 19.9))).toBe(true);
    expect(isIn(v, scale(centreOf(6, cells[0] ?? 0), 25))).toBe(false);
    expect(isIn(v, scale(centreOf(6, cells[0] ?? 0), 5))).toBe(false);
    expect(isIn(v, [0, 0, 15])).toBe(false);
    expect(isIn(v, [-15, 0, 0])).toBe(false);
  });

  it("is the box bounding a member list's members, approximate", () => {
    const members = [{ at: [0, 0, 0] as [number, number, number] }, { at: [2, 4, 6] as [number, number, number] }];
    const v = volume(shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "members", members } }));
    expect(v.approximate).toBe(true);
    expect(isIn(v, [1, 2, 3])).toBe(true);
    expect(isIn(v, [3, 2, 3])).toBe(false);
  });

  it("is the box a point's published extents span about it, approximate", () => {
    const point = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [10, 0, 0] } });
    const extents = {
      frame: "supergalactic",
      axes: [
        [4, 0, 0],
        [0, 2, 0],
        [0, 0, 2],
      ] as [[number, number, number], [number, number, number], [number, number, number]],
    };
    const v = volume(point, { derivation: { extents } });
    expect(v.approximate).toBe(true);
    expect(isIn(v, [11.9, 0.9, -0.9])).toBe(true);
    expect(isIn(v, [12.1, 0, 0])).toBe(false);
    expect(isIn(v, [10, 1.1, 0])).toBe(false);
  });

  it("is the bounding box of a structure with no shape, approximate", () => {
    const v = volume(null, { centre: { x: 0, y: 0, z: 0, unit: "kiloparsec" }, bounds: { min: [-1000, -1000, -1000], max: [1000, 2000, 1000] } });
    expect(v.approximate).toBe(true);
    expect(isIn(v, [0, 1.9, 0])).toBe(true);
    expect(isIn(v, [0, 2.1, 0])).toBe(false);
  });

  it("is none for a structure drawn as a point, a path, a ray or a range alone, or with neither a shape nor bounds", () => {
    const point = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [1, 0, 0] } });
    const path = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "path", points: [[0, 0, 0], [1, 1, 1]] } });
    const ray = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "ray", origin: [0, 0, 0], direction: [1, 0, 0], reach: 5 } });
    const range = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "range", direction: [1, 0, 0], near: 1, far: 5 } });
    for (const shape of [point, path, ray, range]) expect(volumeOf(detail(shape), ROTATIONS)).toBeUndefined();
    expect(volumeOf(detail(null, { bounds: null }), ROTATIONS)).toBeUndefined();
  });

  it("holds a point only where the structure's shape gives the volume exactly, not an approximate one", () => {
    const sphere = volume(shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "sphere", centre: [0, 0, 0], radius: 2 } } }));
    expect(holds(sphere, [1, 0, 0])).toBe(true);
    expect(holds(sphere, [3, 0, 0])).toBe(false);
    // A void published as a point with its bounds is culled to the box, which holds nothing.
    const box = volume(null, { bounds: { min: [0, 0, 0], max: [1, 1, 1] } });
    expect(inside(box, 0.5, 0.5, 0.5)).toBe(true);
    expect(holds(box, [0.5, 0.5, 0.5])).toBe(false);
    // Nor are a void's extents laid about its deepest point, where its paper gives no centre.
    const laid = volume(shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [0, 0, 0] } }), {
      derivation: { extents: { frame: "icrs", axes: [[2, 0, 0], [0, 2, 0], [0, 0, 2]] } },
    } as Partial<StructureDetail>);
    expect(laid.approximate).toBe(true);
    expect(inside(laid, 0.5, 0, 0)).toBe(true);
    expect(holds(laid, [0.5, 0, 0])).toBe(false);
  });

  it("marks each of a tile's points inside or not, by its offset from the tile's centre", () => {
    const v = volume(null, { bounds: { min: [0, 0, 0], max: [1, 1, 1] } });
    const offsets = new Float32Array([0, 0, 0, 0.6, 0, 0, -0.4, 0.4, 0.4]);
    expect([...insideOf(v, [0.5, 0.5, 0.5], offsets, { centre: [0.5, 0.5, 0.5], radius: 1 })]).toEqual([1, 0, 1]);
    // A tile whose points' sphere misses the volume's box has none inside, untested.
    expect([...insideOf(v, [0.5, 0.5, 0.5], offsets, { centre: [5, 5, 5], radius: 1 })]).toEqual([0, 0, 0]);
  });

  it("is met by a tile's cube that reaches its bounding box, and by no other", () => {
    const v = volume(null, { bounds: { min: [0, 0, 0], max: [1, 1, 1] } });
    expect(meets(v.bounds, [2, 0.5, 0.5], 2)).toBe(true);
    expect(meets(v.bounds, [2.1, 0.5, 0.5], 2)).toBe(false);
    expect(meets(v.bounds, [0.5, 0.5, 0.5], 0.1)).toBe(true);
  });
});
