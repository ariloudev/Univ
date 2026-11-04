import { describe, expect, it } from "vitest";

import { POINT_BUDGET, type Tile, apparentSize, nextTiles } from "./lod.ts";

function tile(id: string, centre: [number, number, number], edge: number, points = 10): Tile {
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
  tile("web/r7", [250, 250, 250], 500),
  tile("web/r70", [125, 125, 125], 250),
];

describe("choosing tiles", () => {
  it("fetches the root of every scale domain before any other tile", () => {
    const first = nextTiles(TILES, new Set(), new Set(), 0, [0, 0, 0], 10);
    expect(first.map((t) => t.id)).toEqual(["web/r", "stars/r"]);
  });

  it("fetches the children of held tiles, the largest on screen first", () => {
    const held = new Set(["web/r", "stars/r"]);
    const next = nextTiles(TILES, held, new Set(), 0, [300, 300, 300], 10);
    expect(next.map((t) => t.id)).toEqual(["web/r7", "web/r0"]);
    expect(apparentSize(TILES[3] as Tile, [300, 300, 300])).toBeGreaterThan(apparentSize(TILES[2] as Tile, [300, 300, 300]));
  });

  it("waits for a tile's parent, and fetches even the smallest on screen in the end", () => {
    const held = new Set(["web/r", "stars/r"]);
    expect(nextTiles(TILES, held, new Set(), 0, [300, 300, 300], 10).map((t) => t.id)).not.toContain("web/r70");
    expect(nextTiles(TILES, held, new Set(), 0, [1e6, 0, 0], 10).map((t) => t.id)).toEqual(["web/r7", "web/r0"]);
  });

  it("holds no more than 3,000,000 points", () => {
    expect(POINT_BUDGET).toBe(3_000_000);
    const held = new Set(["web/r", "stars/r"]);
    const next = nextTiles(TILES, held, new Set(), POINT_BUDGET - 15, [300, 300, 300], 10);
    // Room for 15 more points: the larger child (10 points) fits, the next would not.
    expect(next.map((t) => t.id)).toEqual(["web/r7"]);
  });

  it("asks for no tile already held or on its way", () => {
    const next = nextTiles(TILES, new Set(["stars/r"]), new Set(["web/r"]), 0, [0, 0, 0], 10);
    expect(next).toEqual([]);
  });
});
