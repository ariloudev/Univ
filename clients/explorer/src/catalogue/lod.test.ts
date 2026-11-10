import { describe, expect, it } from "vitest";

import { type Sight, sightOf } from "../scene/frustum.ts";
import { IDENTITY, type Vec3 } from "../scene/vec.ts";
import {
  type Extent,
  HELD_PREFERENCE,
  type Holding,
  type Levels,
  HELD_VIEW_MARGIN,
  KEEP_SPACING,
  POINT_BUDGET,
  REFINE_SPACING,
  type Tile,
  type Tree,
  apparentSize,
  extentOf,
  detailOf,
  drawnShare,
  neededAt,
  onsetsOf,
  plan,
  planEvery,
  rankOf,
  spacing,
  treeOf,
} from "./lod.ts";

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
    slice: [0, 1],
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
const AXES: Sight["axes"] = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
];

const sight = (eye: Vec3, all = false): Sight => ({
  eye,
  axes: AXES,
  focal: 1000,
  inView: (centre) => all || centre[0] >= 0,
});

function holding(settled: string[], points = 0, pending: string[] = [], extents: Record<string, Extent> = {}): Holding {
  const drawn = new Map(settled.map((id) => [id, { ...(extents[id] === undefined ? {} : { extent: extents[id] }) }]));
  return { settled: new Set(settled), drawn, pending: new Set(pending), points };
}

const ids = (tiles: readonly Tile[]) => tiles.map((t) => t.id);
/** The levels of the web's tiles of `TILES`. */
const WEB = treeOf(TILES).levels.get("web") as Levels;
const ROOTS = ["web/r", "stars/r"];

/** The eight children of the cube `address` of `domain` about `centre` with edge `edge`, those
 * whose centre `keep` accepts, each of `points` points. */
const children = (id: string, centre: Vec3, edge: number, points: number, keep = (_: Vec3) => true): Tile[] =>
  Array.from({ length: 8 }, (_, i): [number, Vec3] => [
    i,
    [centre[0] + (((i & 1) * 2 - 1) * edge) / 4, centre[1] + (((i & 2) - 1) * edge) / 4, centre[2] + (((i & 4) / 2 - 1) * edge) / 4],
  ])
    .filter(([, c]) => keep(c))
    .map(([i, c]) => tile(`${id}${String(i)}`, c, edge / 2, points));

/** A catalogue that draws each tile as soon as it is fetched, on `tree` within `budget`. */
function simulatedCatalogue(tree: Tree<Tile>, budget = POINT_BUDGET) {
  const settled = new Set<string>();
  const drawn = new Map<string, { readonly extent?: Extent }>();
  const pending = new Set<string>();
  const fetched: string[] = [];
  let points = 0;
  return {
    fetched,
    points: () => points,
    /** Fetches and drops what `sight` asks for, `count` families a round, until it asks for
     * nothing: how many tiles that fetched and dropped. */
    settle(sight: Sight, count = 4): number {
      let moved = 0;
      for (let round = 0; round < 1000; round += 1) {
        const next = plan(tree, { settled, drawn, pending, points }, sight, count, budget);
        if (next.fetch.length === 0 && next.drop.length === 0) return moved;
        for (const t of next.drop) {
          settled.delete(t.id);
          drawn.delete(t.id);
          points -= t.points;
        }
        for (const t of next.fetch) {
          settled.add(t.id);
          drawn.set(t.id, {});
          points += t.points;
          fetched.push(t.id);
        }
        moved += next.fetch.length + next.drop.length;
      }
      throw new Error("the view did not settle");
    },
  };
}

describe("choosing tiles", () => {
  it("fetches the root of every scale domain before any other tile", () => {
    const first = plan(treeOf(TILES), holding([]), sight([0, 0, 0]), 10);
    expect(ids(first.fetch)).toEqual(ROOTS);
  });

  it("fetches the children in view of held tiles, the largest on screen first, and none wholly outside it", () => {
    const eye: Vec3 = [300, 300, 300];
    const next = plan(treeOf(TILES), holding(ROOTS), sight(eye), 10);
    expect(ids(next.fetch)).toEqual(["web/r7", "web/r1"]);
    expect(apparentSize(TILES[4] as Tile, eye)).toBeGreaterThan(apparentSize(TILES[3] as Tile, eye));
  });

  it("refines a tile only while a typical tile of its level would have its points farther apart on screen than 3 pixels", () => {
    expect(REFINE_SPACING).toBe(3);
    const far = sight([1e6, 0, 0]);
    expect(spacing(TILES[0] as Tile, WEB, far)).toBeLessThan(REFINE_SPACING);
    expect(plan(treeOf(TILES), holding(ROOTS), far, 10).fetch).toEqual([]);
    // Nearer, the same tile's points spread apart, and its children are wanted.
    expect(ids(plan(treeOf(TILES), holding(ROOTS), sight([3000, 0, 0]), 10).fetch)).toContain("web/r1");
  });

  it("measures a tile read by the distance of its points, not of its cube", () => {
    // The web's root is 1000 Mpc across, but its points lie within 10 Mpc of its centre.
    const root = TILES[0] as Tile;
    const extent = extentOf(new Float32Array([-10, 0, 0, 10, 0, 0, 0, -5, 5]), root.centre);
    // The box of the points spans x −10 to 10, y −5 to 0 and z 0 to 5.
    expect(extent.centre).toEqual([0, -2.5, 2.5]);
    expect(extent.radius).toBeCloseTo(Math.hypot(10, 2.5, 2.5));
    // A typical root of 10 points 1000 Mpc across has its points 3 px apart from 105,409 Mpc: its
    // cube reaches nearer than that from 106,000 Mpc, its points do not.
    const from = sight([106_000, 0, 0]);
    expect(spacing(root, WEB, from)).toBeGreaterThan(REFINE_SPACING);
    expect(spacing(root, WEB, from, extent)).toBeLessThan(REFINE_SPACING);
    const held = holding(ROOTS, 0, [], { "web/r": extent });
    expect(plan(treeOf(TILES), held, from, 10).fetch).toEqual([]);
    expect(ids(plan(treeOf(TILES), holding(ROOTS), from, 10).fetch)).toContain("web/r1");
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
    expect(spacing(TILES[0] as Tile, WEB, from, extent)).toBeLessThan(spacing(TILES[0] as Tile, WEB, from, { ...extent, core: extent }));
  });

  it("refines regions at one distance alike, however many points their tiles hold", () => {
    // Two split tiles of one level, of 1,000 and of 10 points, as far from the eye: both are
    // measured as the level's typical tile, of 505 points, so both are refined or neither.
    const tiles = [
      tile("web/r", [0, 0, 0], 1000),
      tile("web/r0", [-250, -250, 250], 500, 1000),
      tile("web/r1", [250, -250, 250], 500, 10),
      ...children("web/r0", [-250, -250, 250], 500, 10),
      ...children("web/r1", [250, -250, 250], 500, 10),
    ];
    const tree = treeOf(tiles);
    const levels = tree.levels.get("web") as Levels;
    expect(levels.points[1]).toBe(505);
    const eye: Vec3 = [0, -250, 4000];
    const [dense, sparse] = [tiles[1] as Tile, tiles[2] as Tile];
    expect(spacing(dense, levels, sight(eye, true))).toBeCloseTo(spacing(sparse, levels, sight(eye, true)));
    const next = plan(tree, holding(["web/r", "web/r0", "web/r1"], 1030), sight(eye, true), 10);
    const parents = new Set(next.fetch.map((t) => t.parent));
    expect(parents).toEqual(new Set(["web/r0", "web/r1"]));
  });

  it("holds no more than 6,000,000 points, counting those on their way", () => {
    expect(POINT_BUDGET).toBe(6_000_000);
    // The root's children in view, r7 and r1, come as a family of 20 points: with room for 25
    // they are fetched, with room for 15 and nothing to drop they are not.
    expect(ids(plan(treeOf(TILES), holding(ROOTS, POINT_BUDGET - 25), sight([300, 300, 300]), 10).fetch)).toEqual(["web/r7", "web/r1"]);
    const next = plan(treeOf(TILES), holding(ROOTS, POINT_BUDGET - 15), sight([300, 300, 300]), 10);
    expect(next).toEqual({ fetch: [], drop: [] });
  });

  it("asks for no tile already held or on its way", () => {
    const next = plan(treeOf(TILES), holding(["stars/r"], 0, ["web/r"]), sight([0, 0, 0]), 10);
    expect(next.fetch).toEqual([]);
  });
});

describe("a view that does not change", () => {
  it("settles, within a budget too small for all it wants, and fetches no tile twice", () => {
    // Eight children of 10 points about a root of 10, all in view; room for the root and three.
    const tiles = [tile("web/r", [0, 0, 0], 1000), ...children("web/r", [0, 0, 0], 1000, 10)];
    const catalogue = simulatedCatalogue(treeOf(tiles), 40);
    catalogue.settle(sight([900, 300, 100], true), 2);
    expect(catalogue.points()).toBeLessThanOrEqual(40);
    expect(new Set(catalogue.fetched).size).toBe(catalogue.fetched.length);
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
    expect(plan(treeOf(family), holding(["web/r"], 10), sight(eye), 10, 40).fetch).toEqual([]);
    expect(ids(plan(treeOf(family), holding(["web/r"], 10), sight(eye), 10, 45).fetch).sort()).toEqual(["web/r1", "web/r5", "web/r7"]);
    // A family is fetched whole even where it holds more tiles than may start at once.
    expect(ids(plan(treeOf(family), holding(["web/r"], 10), sight(eye), 1, 45).fetch)).toHaveLength(3);
  });
});

describe("making room", () => {
  it("drops a held tile once its children are dropped, when its leaves alone do not make room", () => {
    // The view has turned from r0 (held, with a child held) to r7: r7 needs more room than the
    // leaf r00 frees, so r0 goes too, in the same plan.
    const tiles = [
      tile("web/r", [0, 0, 0], 1000),
      tile("web/r0", [-250, -250, -250], 500, 10),
      tile("web/r00", [-375, -375, -375], 250, 5),
      tile("web/r7", [250, 250, 250], 500, 14),
    ];
    const next = plan(treeOf(tiles), holding(["web/r", "web/r0", "web/r00"], 25), sight([300, 300, 300]), 6, 30);
    expect(ids(next.drop)).toEqual(["web/r00", "web/r0"]);
    expect(ids(next.fetch)).toEqual(["web/r7"]);
  });

  it("drops nothing where even dropping all it may would not make room", () => {
    const tiles = [tile("web/r", [0, 0, 0], 1000), tile("web/r0", [-250, -250, -250], 500, 10), tile("web/r7", [250, 250, 250], 500, 40)];
    const next = plan(treeOf(tiles), holding(["web/r", "web/r0"], 20), sight([300, 300, 300]), 6, 45);
    expect(next).toEqual({ fetch: [], drop: [] });
  });

  it("may drop a tile whose children failed their check, as none of them is drawn", () => {
    const tiles = [
      tile("web/r", [0, 0, 0], 1000),
      tile("web/r0", [-250, -250, -250], 500, 10),
      tile("web/r00", [-375, -375, -375], 250, 5),
      tile("web/r7", [250, 250, 250], 500, 10),
    ];
    // r00 is settled but not drawn: it failed.
    const held: Holding = { ...holding(["web/r", "web/r0"], 20), settled: new Set(["web/r", "web/r0", "web/r00"]) };
    expect(ids(plan(treeOf(tiles), held, sight([300, 300, 300]), 6, 25).drop)).toEqual(["web/r0"]);
  });

  const eye: Vec3 = [300, 300, 300];
  const full = (settled: string[]) => holding(settled, POINT_BUDGET - 5);

  it("drops a tile wholly outside the view to fetch one in it", () => {
    const next = plan(treeOf(TILES), full([...ROOTS, "web/r0", "web/r1", "web/r7"]), sight(eye), 10);
    expect(ids(next.drop)).toEqual(["web/r0"]);
    expect(ids(next.fetch)).toEqual(["web/r70"]);
  });

  it("never drops a root, nor a tile whose children are held, nor the tile being refined", () => {
    const next = plan(treeOf(TILES), full([...ROOTS, "web/r7", "web/r70"]), sight(eye), 10);
    // r7's child is held, r70 is wanted no more than what would replace it, and the roots stay.
    expect(next.drop.map((t) => t.id)).not.toContain("web/r");
    expect(next.drop.map((t) => t.id)).not.toContain("web/r7");
  });

  it("drops no tile the view still wants, even to fetch another it wants", () => {
    const next = plan(treeOf(TILES), full([...ROOTS, "web/r1", "web/r7"]), sight(eye), 10);
    expect(next.drop).toEqual([]);
    expect(next.fetch).toEqual([]);
  });

  it("fetches a dropped tile again once the view needs it", () => {
    const back = plan(treeOf(TILES), holding([...ROOTS, "web/r1", "web/r7"]), sight(eye, true), 10);
    expect(ids(back.fetch)).toContain("web/r0");
  });
});

describe("a view that moves a little", () => {
  it("keeps a held tile's children while its points lie farther apart on screen than 2 pixels", () => {
    expect(KEEP_SPACING).toBe(2);
    // The web's root seen from where its points lie 2.5 pixels apart: below the spacing that
    // refines it, above the one that keeps its children.
    const root = tile("web/r", [0, 0, 0], 1000);
    const eye: Vec3 = [500 * Math.sqrt(3) + (1000 * 1000) / Math.sqrt(10) / 2.5, 0, 0];
    expect(spacing(root, WEB, sight(eye))).toBeCloseTo(2.5);
    const held = children("web/r", [0, 0, 0], 1000, 10);
    // A tile near the eye that the view wants, and a held one it does not (its parent's points
    // too crowded on screen to refine), larger on screen than the web's children.
    const near = tile("near/r", [eye[0] - 20, 30, 0], 10);
    const wanted = tile("near/r0", [eye[0] - 22.5, 27.5, -2.5], 5);
    const dense = tile("dense/r", [eye[0] - 50, -30, 0], 20, 1e8);
    const unwanted = tile("dense/r0", [eye[0] - 55, -35, -5], 10);
    expect(apparentSize(unwanted, eye)).toBeGreaterThan(apparentSize(held[0] as Tile, eye));
    const holds = [root, ...held, near, dense, unwanted];
    const points = holds.reduce((sum, t) => sum + t.points, 0);
    const next = plan(treeOf([...holds, wanted]), holding(ids(holds), points), sight(eye, true), 4, points);
    // Room for the wanted tile is made from the tile the view does not want, the web's
    // children kept.
    expect(ids(next.fetch)).toEqual(["near/r0"]);
    expect(ids(next.drop)).toEqual(["dense/r0"]);
  });

  describe("with room for one of two families", () => {
    // The web's root and two children held; the first child's two children held, the second's
    // not. The budget holds the root, its children and one family.
    const tiles = [
      tile("web/r", [0, 0, 0], 1000, 1),
      tile("web/r0", [-250, -250, -250], 500, 1),
      tile("web/r00", [-375, -375, -375], 250, 1),
      tile("web/r07", [-125, -125, -125], 250, 1),
      tile("web/r7", [250, 250, 250], 500, 1),
      tile("web/r70", [125, 125, 125], 250, 1),
      tile("web/r77", [375, 375, 375], 250, 1),
    ];
    const held = holding(["web/r", "web/r0", "web/r00", "web/r07", "web/r7"], 5);
    const largest = (eye: Vec3, family: string) =>
      Math.max(...tiles.filter((t) => t.parent === family).map((t) => apparentSize(t, eye)));

    it("keeps the held family where the other is less than half again its size on screen", () => {
      expect(HELD_PREFERENCE).toBe(1.5);
      const eye: Vec3 = [3e4, 3e4, 3e4];
      expect(largest(eye, "web/r7")).toBeGreaterThan(largest(eye, "web/r0"));
      expect(largest(eye, "web/r7")).toBeLessThan(HELD_PREFERENCE * largest(eye, "web/r0"));
      expect(plan(treeOf(tiles), held, sight(eye, true), 4, 5)).toEqual({ fetch: [], drop: [] });
    });

    it("prefers a family that holds only some of its tiles in view", () => {
      // The first family's second tile is not held: the family still counts half again its
      // size, so the view fetches that tile rather than trading the family for the other.
      const eye: Vec3 = [3e4, 3e4, 3e4];
      const part = holding(["web/r", "web/r0", "web/r00", "web/r7"], 4);
      expect(plan(treeOf(tiles), part, sight(eye, true), 4, 5)).toEqual({ fetch: [tiles[3]], drop: [] });
    });

    it("trades it for the other once that is more than half again its size on screen", () => {
      const eye: Vec3 = [700, 700, 700];
      expect(largest(eye, "web/r7")).toBeGreaterThan(HELD_PREFERENCE * largest(eye, "web/r0"));
      const next = plan(treeOf(tiles), held, sight(eye, true), 4, 5);
      expect(ids(next.fetch).sort()).toEqual(["web/r70", "web/r77"]);
      expect(ids(next.drop).sort()).toEqual(["web/r00", "web/r07"]);
    });
  });

  it("settles when turned by 0.1 degree and back again and again, its budget full, and follows a larger turn", () => {
    expect(HELD_VIEW_MARGIN).toBeCloseTo(Math.PI / 180);
    // SIMULATED: an octree five levels deep over a cloud 1,500 Mpc in radius, 800 points a tile,
    // more than a budget of 3,000,000 can hold of what the view wants, seen from 1,500 Mpc off a
    // focus 670 Mpc from its centre, so that the view's sides cut through it.
    const budget = 3_000_000;
    const tiles: Tile[] = [tile("web/r", [0, 0, 0], 4000, 800)];
    const inCloud = (edge: number) => (c: Vec3) => Math.hypot(...c) - edge * 0.433 <= 1500;
    for (let level = 0, edge = 4000; level < 5; level += 1, edge /= 2) {
      for (const parent of tiles.filter((t) => t.edge === edge)) tiles.push(...children(parent.id, parent.centre, edge, 800, inCloud(edge)));
    }
    expect(tiles.length * 800).toBeGreaterThan(POINT_BUDGET);
    const catalogue = simulatedCatalogue(treeOf(tiles), budget);
    const turned = (yaw: number) => sightOf({ focus: [600, 300, 0], distance: 1500, yaw, pitch: 0.2 }, IDENTITY, 1000, 700);
    const turn = (0.1 * Math.PI) / 180;
    catalogue.settle(turned(0.3));
    // Full: no family of eight more tiles fits.
    expect(catalogue.points()).toBeGreaterThan(budget - 8 * 800);
    expect(catalogue.points()).toBeLessThanOrEqual(budget);
    catalogue.settle(turned(0.3 + turn));
    const after = Array.from({ length: 4 }, () => catalogue.settle(turned(0.3)) + catalogue.settle(turned(0.3 + turn)));
    expect(after).toEqual([0, 0, 0, 0]);
    expect(catalogue.settle(turned(0.3 + (30 * Math.PI) / 180))).toBeGreaterThan(0);
  });
});

describe("level of detail turned off", () => {
  it("fetches every tile, the coarser levels first, whatever the view and the budget, and drops none", () => {
    // The plan takes no view, and the budget is already spent twice over.
    expect(planEvery(treeOf(TILES), holding([], POINT_BUDGET * 2), 3)).toEqual({ fetch: [TILES[0], TILES[1], TILES[2]], drop: [] });
    expect(ids(planEvery(treeOf(TILES), holding(ROOTS, POINT_BUDGET * 2), 10).fetch)).toEqual(["web/r0", "web/r1", "web/r7", "web/r70"]);
    expect(ids(planEvery(treeOf(TILES), holding(ROOTS, 0, ["web/r1"]), 2).fetch)).toEqual(["web/r0", "web/r7"]);
    expect(planEvery(treeOf(TILES), holding(ids(TILES)), 10)).toEqual({ fetch: [], drop: [] });
  });

  it("is followed, once turned on again, by dropping what the view does not want until the points held fit the budget", () => {
    // Every tile held, 60 points, within a budget of 30: the view at x ≥ 0 wants none of the
    // root's children within it, so r0 (out of view) goes first, then the smaller r70, then r1.
    const next = plan(treeOf(TILES), holding(ids(TILES), 60), sight([300, 300, 300]), 6, 30);
    expect(next.fetch).toEqual([]);
    expect(ids(next.drop)).toEqual(["web/r0", "web/r70", "web/r1"]);
  });
});

describe("the share of a domain drawn", () => {
  // Three levels: a root keeping a tenth of the domain, a level adding three tenths, and leaves
  // holding the rest; typical split tiles of 100 points, 1000 and 500 Mpc across. Seen with a
  // focal length of 1000 px, level 1 is needed within 33,333 Mpc and level 2 within 16,667 Mpc.
  const levels: Levels = { edges: [1000, 500, 250], points: [100, 100, 0], cumulative: [0.1, 0.4, 1] };
  const onsets = onsetsOf(levels, 1000);
  const detail = detailOf(levels, onsets);
  const parent = tile("web/r", [0, 0, 0], 1000, 100);

  it("rises level by level as a typical tile of the level above has its points from 3 to 6 pixels apart", () => {
    expect(onsets[1]).toBeCloseTo(100_000 / 3);
    expect(onsets[2]).toBeCloseTo(50_000 / 3);
    expect(spacing(parent, levels, { ...sight([0, 0, 0]), eye: [onsets[1] ?? 0, 0, 0] }, { centre: [0, 0, 0], radius: 0, core: { centre: [0, 0, 0], radius: 0 } })).toBeCloseTo(3);
    expect(neededAt(detail, 1e6)).toBeCloseTo(0.1);
    expect(neededAt(detail, 100_000 / 3)).toBeCloseTo(0.1);
    expect(neededAt(detail, 200_000 / 9)).toBeCloseTo(0.25);
    expect(neededAt(detail, 50_000 / 3)).toBeCloseTo(0.4);
    expect(neededAt(detail, 100_000 / 9)).toBeCloseTo(0.7);
    expect(neededAt(detail, 25_000 / 3)).toBeCloseTo(1);
    expect(neededAt(detail, 1)).toBeCloseTo(1);
  });

  it("ranks a tile's points evenly through its slice, in their order", () => {
    const child = { ...tile("web/r0", [0, 0, 0], 500, 3), slice: [0.1, 0.4] as const };
    expect([0, 1, 2].map((i) => rankOf(child, i, 3))).toEqual([0.15, 0.25, 0.35].map((r) => expect.closeTo(r, 10)));
  });

  it("draws every point of a root, and of a tile the first whose rank lies below the share its nearest points need", () => {
    const root = { ...parent, slice: [0, 0.1] as const };
    const child = { ...tile("web/r0", [0, 0, 0], 500, 100), slice: [0.1, 0.4] as const };
    const from = (distance: number): Vec3 => [distance, 0, 0];
    const at = { centre: [0, 0, 0] as Vec3, radius: 0 };
    expect(drawnShare(root, detail, from(1e9), at)).toBe(1);
    expect(drawnShare(child, detail, from(1e6), at)).toBeCloseTo(0, 6);
    expect(drawnShare(child, detail, from(200_000 / 9), at)).toBeCloseTo(0.5);
    expect(drawnShare(child, detail, from(1000), at)).toBe(1);
  });
});
