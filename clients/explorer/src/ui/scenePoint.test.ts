// @vitest-environment node
import { describe, expect, it } from "vitest";

import { MPC_PER, tilesOf } from "../catalogue/tiles.ts";
import { manifestOf } from "../test/fixtures.ts";
import { pointOf } from "./scenePoint.ts";

describe("a drawn point", () => {
  it("is known by its id, scale domain, place and distance, and the source and kind of its distance", () => {
    const manifest = manifestOf("stars");
    const tile = tilesOf("stars", manifest)[0];
    const source = manifest.sources[0];
    if (tile === undefined || source === undefined) throw new Error("no fixture tile");
    const domain = { name: "stars", manifest, toMpc: MPC_PER.pc, tiles: [tile] };
    const batch = {
      tile,
      offsets: new Float32Array([3e-6 - tile.centre[0], 4e-6 - tile.centre[1], -tile.centre[2]]),
      kinds: new Uint8Array([1]),
      sources: new Uint16Array([source.index]),
      ids: new Float64Array([42]),
    };
    const point = pointOf(new Map([["stars", domain]]), batch, 0);
    expect(point.id).toBe(42);
    expect(point.domain).toBe("stars");
    expect(point.distanceMpc).toBeCloseTo(5e-6, 10);
    expect(point.source).toBe(source.title);
    expect(point.kind).toBe("estimated");
  });
});
