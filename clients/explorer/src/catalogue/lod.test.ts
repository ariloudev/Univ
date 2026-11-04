import { describe, expect, it } from "vitest";

import type { Sight } from "../scene/frustum.ts";
import type { Vec3 } from "../scene/vec.ts";
import { type Holding, POINT_BUDGET, REFINE_SPACING, type Tile, apparentSize, extentOf, plan, spacing } from "./lod.ts";

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

  it("measures a tile read by where its points lie, not by its cube", () => {
    // The web's root is 1000 Mpc across, but its points lie within 10 Mpc of its centre.
    const root = TILES[0] as Tile;
    const extent = extentOf(new Float32Array([-10, 0, 0, 10, 0, 0, 0, -5, 5]), root.centre);
    // The box of the points spans x −10 to 10, y −5 to 0 and z 0 to 5.
    expect(extent.centre).toEqual([0, -2.5, 2.5]);
    expect(extent.radius).toBeCloseTo(Math.hypot(10, 2.5, 2.5));
    const from = sight([3000, 0, 0]);
    expect(spacing(root, from, extent)).toBeLessThan(spacing(root, from));
    // So seen from 3 Gpc its points already lie closer than 3 px, and it is not refined.
    const held = { ...holding(ROOTS), extent: (id: string) => (id === "web/r" ? extent : undefined) };
    expect(spacing(root, from, extent)).toBeLessThan(REFINE_SPACING);
    expect(plan(TILES, held, from, 10).fetch).toEqual([]);
    expect(extentOf(new Float32Array(0), [1, 2, 3])).toEqual({ centre: [1, 2, 3], radius: 0, core: { centre: [1, 2, 3], radius: 0 } });
  });

  it("measures how far apart a tile's points lie by where most of them are, not by the farthest", () => {
    // Twenty-seven points within 1 Mpc of the centre and one 1000 Mpc out: the box reaches the
    // far one, the core holds the near ones about their mean, 36 Mpc out.
    const near = Array.from({ length: 27 }, (_, i) => [Math.cos(i), Math.sin(i), 0]).flat();
    const extent = extentOf(new Float32Array([...near, 1000, 0, 0]), [0, 0, 0]);
    expect(extent.radius).toBeGreaterThan(500);
    expect(extent.core.radius).toBeLessThan(40);
    const from = sight([5000, 0, 0]);
    expect(spacing(TILES[0] as Tile, from, extent)).toBeLessThan(spacing(TILES[0] as Tile, from, { ...extent, core: extent }));
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

describe("a view that does not change", () => {
  it("settles, within a budget too small for all it wants, and fetches no tile twice", () => {
    // Eight children of 10 points about a root of 10, all in view; room for the root and three.
    const children = Array.from({ length: 8 }, (_, i) =>
      tile(`web/r${i}`, [((i & 1) * 2 - 1) * 250, ((i & 2) - 1) * 250, ((i & 4) / 2 - 1) * 250], 500),
    );
    const tiles = [tile("web/r", [0, 0, 0], 1000), ...children];
    const settled = new Set<string>();
    const fetched: string[] = [];
    let points = 0;
    let rounds = 0;
    for (; rounds < 50; rounds += 1) {
      const next = plan(tiles, { settled, drawn: settled, pending: new Set(), points }, sight([900, 300, 100], true), 2, 40);
      if (next.fetch.length === 0 && next.drop.length === 0) break;
      for (const t of next.drop) {
        settled.delete(t.id);
        points -= t.points;
      }
      for (const t of next.fetch) {
        settled.add(t.id);
        points += t.points;
        fetched.push(t.id);
      }
    }
    expect(rounds).toBeLessThan(50);
    expect(points).toBeLessThanOrEqual(40);
    expect(new Set(fetched).size).toBe(fetched.length);
  });
});

describe("refining a region", () => {
  it("adds a tile's children in view all together, or none, so no octant stays coarser than the rest", () => {
    // Room for the root (10) and 30 more: of two children of 10 and one of 15 in view, none fits
    // as a family of 35, so none is wanted, where one by one the two smaller would have been.
    const family = [
      tile("web/r", [0, 0, 0], 1000),
      tile("web/r1", [250, -250, 250], 500),
      tile("web/r5", [250, -250, -250], 500),
      tile("web/r7", [250, 250, 250], 500, 15),
    ];
    const eye: Vec3 = [3000, 0, 0];
    expect(plan(family, holding(["web/r"], 10), sight(eye), 10, 40).fetch).toEqual([]);
    expect(ids(plan(family, holding(["web/r"], 10), sight(eye), 10, 45).fetch).sort()).toEqual(["web/r1", "web/r5", "web/r7"]);
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

  it("drops no tile the view still wants, even to fetch another it wants", () => {
    const next = plan(TILES, full([...ROOTS, "web/r1", "web/r7"]), sight(eye), 10);
    expect(next.drop).toEqual([]);
    expect(next.fetch).toEqual([]);
  });

  it("fetches a dropped tile again once the view needs it", () => {
    const back = plan(TILES, holding([...ROOTS, "web/r1", "web/r7"]), sight(eye, true), 10);
    expect(ids(back.fetch)).toContain("web/r0");
  });
});
