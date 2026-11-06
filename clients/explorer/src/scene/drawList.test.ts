import { describe, expect, it } from "vitest";

import type { Sight } from "./frustum.ts";
import { type Listed, drawList } from "./drawList.ts";
import { NO_MASKS } from "./masks.ts";
import { IDENTITY, type Vec3 } from "./vec.ts";

const tile = (id: string, domain: string, centre: Vec3, count = 100): Listed => ({ id, domain, count, extent: { centre, radius: 1 } });

/** A sight that sees what lies where x ≥ 0. */
const sight: Sight = {
  eye: [0, 0, 0],
  axes: IDENTITY,
  focal: 1000,
  inView: (centre) => centre[0] >= 0,
};

const TILES = [tile("web/r", "web", [5, 0, 0]), tile("web/r0", "web", [-5, 0, 0]), tile("stars/r", "stars", [1, 0, 0])];
const ids = (list: { batch: Listed }[]) => list.map((d) => d.batch.id);

describe("what a frame draws", () => {
  it("is each tile in sight and not turned off, its share of its points and as opaque as its fade", () => {
    const shares: Record<string, number> = { "web/r": 0.25, "stars/r": 1 };
    const fades: Record<string, number> = { "web/r": 0.5, "stars/r": 1 };
    const list = drawList(TILES, { sight, masks: NO_MASKS, share: (id) => shares[id] ?? 1, fade: (id) => fades[id] ?? 1 });
    expect(list).toEqual([
      { batch: TILES[0], count: 25, fade: 0.5 },
      { batch: TILES[2], count: 100, fade: 1 },
    ]);
  });

  it("leaves out a tile whose share draws no point, or whose fade is nothing", () => {
    expect(ids(drawList(TILES, { sight, masks: NO_MASKS, share: (id) => (id === "web/r" ? 0.001 : 1), fade: () => 1 }))).toEqual(["stars/r"]);
    expect(ids(drawList(TILES, { sight, masks: NO_MASKS, share: () => 1, fade: (id) => (id === "stars/r" ? 0 : 1) }))).toEqual(["web/r"]);
  });

  it("leaves out a domain turned off", () => {
    const masks = { ...NO_MASKS, domains: new Set(["web"]) };
    expect(ids(drawList(TILES, { sight, masks, share: () => 1, fade: () => 1 }))).toEqual(["stars/r"]);
  });
});
