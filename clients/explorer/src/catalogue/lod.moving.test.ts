// A view that moves a little keeps the tiles it holds (draw-the-catalogues): a held tile's
// children are kept down to 2 pixels, a held family is preferred to one of nearly its size, and
// a view turned a little and back, its budget full, settles rather than fetching again and again.
// The octree of the last test is SIMULATED: a cloud of tiles of 400 points each, not a release.

import { describe, expect, it } from "vitest";

import { type Sight, sightOf } from "../scene/frustum.ts";
import type { Mat3, Vec3 } from "../scene/vec.ts";
import { HELD_PREFERENCE, KEEP_SPACING, REFINE_SPACING, type Tile, apparentSize, plan, spacing, treeOf } from "./lod.ts";

function tile(id: string, centre: Vec3, edge: number, points: number): Tile {
  const [domain = "", address = ""] = id.split("/");
  const parent = address.length > 1 ? `${domain}/${address.slice(0, -1)}` : undefined;
  return { id, domain, address, parent, centre, edge, points };
}

/** A sight from `eye` that sees everything. */
const sight = (eye: Vec3): Sight => ({
  eye,
  axes: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  focal: 1000,
  inView: () => true,
});

const held = (tiles: readonly Tile[]) => {
  const ids = tiles.map((t) => t.id);
  return { settled: new Set(ids), drawn: new Map(ids.map((id) => [id, {}])), pending: new Set<string>(), points: tiles.reduce((sum, t) => sum + t.points, 0) };
};

const ids = (tiles: readonly Tile[]) => tiles.map((t) => t.id).sort();

describe("a view that moves a little", () => {
  it("keeps a held tile's children while its points lie farther apart on screen than 2 pixels", () => {
    expect(KEEP_SPACING).toBe(2);
    // The web's root seen from where its points lie 2.5 pixels apart: below the spacing that
    // refines it, above the one that keeps its children.
    const root = tile("web/r", [0, 0, 0], 1000, 10);
    const k = (2 * 500 * Math.sqrt(3) * 1000) / Math.sqrt(10);
    const eye: Vec3 = [500 * Math.sqrt(3) + k / 2.5, 0, 0];
    const at = spacing(root, sight(eye));
    expect(at).toBeGreaterThan(KEEP_SPACING);
    expect(at).toBeLessThan(REFINE_SPACING);
    const children = Array.from({ length: 8 }, (_, i) =>
      tile(`web/r${i}`, [((i & 1) * 2 - 1) * 250, ((i & 2) - 1) * 250, ((i & 4) / 2 - 1) * 250], 500, 10),
    );
    // A tile near the eye that the view wants, and a held one it does not (its parent's points
    // too crowded on screen to refine), larger on screen than the web's children.
    const near = tile("near/r", [eye[0] - 20, 30, 0], 10, 10);
    const wanted = tile("near/r0", [eye[0] - 22.5, 27.5, -2.5], 5, 10);
    const dense = tile("dense/r", [eye[0] - 50, -30, 0], 20, 1e8);
    const unwanted = tile("dense/r0", [eye[0] - 55, -35, -5], 10, 10);
    expect(apparentSize(unwanted, eye)).toBeGreaterThan(apparentSize(children[0] as Tile, eye));
    const holding = held([root, ...children, near, dense, unwanted]);
    const next = plan(treeOf([root, ...children, near, wanted, dense, unwanted]), holding, sight(eye), 4, holding.points);
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
    const holding = held(tiles.filter((t) => !t.id.startsWith("web/r7") || t.id === "web/r7"));
    const largest = (eye: Vec3, family: string) =>
      Math.max(...tiles.filter((t) => t.parent === family).map((t) => apparentSize(t, eye)));

    it("keeps the held family where the other is less than half again its size on screen", () => {
      expect(HELD_PREFERENCE).toBe(1.5);
      const eye: Vec3 = [3e4, 3e4, 3e4];
      expect(largest(eye, "web/r7")).toBeGreaterThan(largest(eye, "web/r0"));
      expect(largest(eye, "web/r7")).toBeLessThan(HELD_PREFERENCE * largest(eye, "web/r0"));
      const next = plan(treeOf(tiles), holding, sight(eye), 4, holding.points);
      expect(next).toEqual({ fetch: [], drop: [] });
    });

    it("trades it for the other once that is more than half again its size on screen", () => {
      const eye: Vec3 = [700, 700, 700];
      expect(largest(eye, "web/r7")).toBeGreaterThan(HELD_PREFERENCE * largest(eye, "web/r0"));
      const next = plan(treeOf(tiles), holding, sight(eye), 4, holding.points);
      expect(ids(next.fetch)).toEqual(["web/r70", "web/r77"]);
      expect(ids(next.drop)).toEqual(["web/r00", "web/r07"]);
    });
  });

  it("settles when turned by 0.1 degree and back again and again, its budget full", () => {
    // SIMULATED: an octree five levels deep over a cloud 1,500 Mpc in radius, 400 points a tile,
    // more than the budget of 3,000,000 can hold, seen from 3,000 Mpc.
    const tiles: Tile[] = [];
    const build = (address: string, centre: Vec3, edge: number, depth: number) => {
      tiles.push(tile(`web/${address}`, centre, edge, 400));
      if (depth === 0) return;
      for (let i = 0; i < 8; i += 1) {
        const c: Vec3 = [centre[0] + (((i & 1) * 2 - 1) * edge) / 4, centre[1] + (((i & 2) - 1) * edge) / 4, centre[2] + (((i & 4) / 2 - 1) * edge) / 4];
        if (Math.hypot(...c) - edge * 0.433 <= 1500) build(address + String(i), c, edge / 2, depth - 1);
      }
    };
    build("r", [0, 0, 0], 4000, 5);
    expect(tiles.length * 400).toBeGreaterThan(3_000_000);
    const tree = treeOf(tiles);
    const identity: Mat3 = [
      [1, 0, 0],
      [0, 1, 0],
      [0, 0, 1],
    ];
    const settled = new Set<string>();
    let points = 0;
    /** Fetches and drops what the view turned to `yaw` asks for until it asks for nothing; how
     * many tiles that took. */
    const settle = (yaw: number): number => {
      const view = sightOf({ focus: [0, 0, 0], distance: 3000, yaw, pitch: 0.2 }, identity, 1000, 700);
      let moved = 0;
      for (let round = 0; round < 1000; round += 1) {
        const next = plan(tree, { settled, drawn: new Map([...settled].map((id) => [id, {}])), pending: new Set(), points }, view, 4);
        if (next.fetch.length === 0 && next.drop.length === 0) return moved;
        for (const t of next.drop) {
          settled.delete(t.id);
          points -= t.points;
        }
        for (const t of next.fetch) {
          settled.add(t.id);
          points += t.points;
        }
        moved += next.fetch.length + next.drop.length;
      }
      throw new Error("the view did not settle");
    };
    const turn = (0.1 * Math.PI) / 180;
    settle(0.3);
    expect(points).toBe(3_000_000);
    settle(0.3 + turn);
    const after = Array.from({ length: 4 }, () => settle(0.3) + settle(0.3 + turn));
    expect(after).toEqual([0, 0, 0, 0]);
  });
});
