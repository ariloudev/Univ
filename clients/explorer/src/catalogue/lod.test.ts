import { describe, expect, it } from "vitest";

import type { Sight } from "../scene/frustum.ts";
import type { Vec3 } from "../scene/vec.ts";
import { type Holding, POINT_BUDGET, REFINE_SPACING, type Tile, apparentSize, plan, spacing } from "./lod.ts";

function tile(id: string, centre: Vec3, edge: number, points = 10): Tile {
  const [domain = "", address = ""] = id.split("/");
  return {
    id,
    domain,
    address,
    parent: address.length > 1 ? `${domain}/${address.slice(0, -1)}` : undefined,
    centre,
    edge,
    points,
  };
}

const TILES = [
  tile("web/r", [0, 0, 0], 1000),
  tile("stars/r", [0, 0, 0], 2e-3),
  tile("web/r0", [-250, -250, -250], 500),
  tile("web/r1", [250, -250, 250], 500),
  tile("web/r7", [250, 250, 250], 500),
  tile("web/r70", [125, 125, 125], 250),
];

/** A sight from `eye` that sees the tiles centred where x ≥ 0, and everything where `all`. */
const sight = (eye: Vec3, all = false): Sight => ({
  eye,
  focal: 1000,
  inView: (centre) => all || centre[0] >= 0,
});

function holding(settled: string[], points = 0, pending: string[] = []): Holding {
  return { settled: new Set(settled), drawn: new Set(settled), pending: new Set(pending), points };
}

const ids = (tiles: readonly Tile[]) => tiles.map((t) => t.id);
const ROOTS = ["web/r", "stars/r"];

describe("choosing tiles", () => {
  it("fetches the root of every scale domain before any other tile", () => {
    const first = plan(TILES, holding([]), sight([0, 0, 0]), 10);
    expect(ids(first.fetch)).toEqual(ROOTS);
  });

  it("fetches the children in view of held tiles, the largest on screen first, and none wholly outside it", () => {
    const eye: Vec3 = [300, 300, 300];
    const next = plan(TILES, holding(ROOTS), sight(eye), 10);
    expect(ids(next.fetch)).toEqual(["web/r7", "web/r1"]);
    expect(apparentSize(TILES[4] as Tile, eye)).toBeGreaterThan(apparentSize(TILES[3] as Tile, eye));
  });

  it("refines a tile only while its points lie farther apart on screen than 3 pixels", () => {
    expect(REFINE_SPACING).toBe(3);
    const far = sight([1e6, 0, 0]);
    expect(spacing(TILES[0] as Tile, far)).toBeLessThan(REFINE_SPACING);
    expect(plan(TILES, holding(ROOTS), far, 10).fetch).toEqual([]);
    // Nearer, the same tile's points spread apart, and its children are wanted.
    expect(ids(plan(TILES, holding(ROOTS), sight([3000, 0, 0]), 10).fetch)).toContain("web/r1");
  });

  it("holds no more than 3,000,000 points, counting those on their way", () => {
    expect(POINT_BUDGET).toBe(3_000_000);
    // Room for 15 more points and nothing to drop: one child of 10 points fits, the next would not.
    const next = plan(TILES, holding(ROOTS, POINT_BUDGET - 15), sight([300, 300, 300]), 10);
    expect(ids(next.fetch)).toEqual(["web/r7"]);
    expect(next.drop).toEqual([]);
  });

  it("asks for no tile already held or on its way", () => {
    const next = plan(TILES, holding(["stars/r"], 0, ["web/r"]), sight([0, 0, 0]), 10);
    expect(next.fetch).toEqual([]);
  });
});

describe("making room", () => {
  const eye: Vec3 = [300, 300, 300];
  const full = (settled: string[]) => holding(settled, POINT_BUDGET - 5);

  it("drops a tile wholly outside the view to fetch one in it", () => {
    const next = plan(TILES, full([...ROOTS, "web/r0", "web/r1", "web/r7"]), sight(eye), 10);
    expect(ids(next.drop)).toEqual(["web/r0"]);
    expect(ids(next.fetch)).toEqual(["web/r70"]);
  });

  it("never drops a root, nor a tile whose children are held, nor the tile being refined", () => {
    const next = plan(TILES, full([...ROOTS, "web/r7", "web/r70"]), sight(eye), 10);
    // r7's child is held, r70 is wanted no more than what would replace it, and the roots stay.
    expect(next.drop.map((t) => t.id)).not.toContain("web/r");
    expect(next.drop.map((t) => t.id)).not.toContain("web/r7");
  });

  it("keeps a tile in view that is larger on screen than the one that would replace it", () => {
    const next = plan(TILES, full([...ROOTS, "web/r1", "web/r7"]), sight(eye), 10);
    expect(next.drop).toEqual([]);
    expect(next.fetch).toEqual([]);
  });

  it("fetches a dropped tile again once the view needs it", () => {
    const back = plan(TILES, holding([...ROOTS, "web/r1", "web/r7"]), sight(eye, true), 10);
    expect(ids(back.fetch)).toContain("web/r0");
  });
});
