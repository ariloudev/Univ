// @vitest-environment node
import { describe, expect, it } from "vitest";

import { MPC_PER, tilesOf } from "../catalogue/tiles.ts";
import { manifestOf } from "../test/fixtures.ts";
import { fakeWorld } from "../test/world.ts";
import { pointOf, pointUnder } from "./scenePoint.ts";

function starBatch() {
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
    extent: { centre: [0, 0, 0] as [number, number, number], radius: 1, core: { centre: [0, 0, 0] as [number, number, number], radius: 1 } },
  };
  return { domain, batch, source };
}

describe("a drawn point", () => {
  it("is found under the pointer, or nothing is", () => {
    const { domain, batch } = starBatch();
    const world = fakeWorld({ catalogue: { domain: () => domain } as never });
    world.pickable.push({ centre: batch.tile.centre, radius: 1, offsets: batch.offsets, batch });
    world.controller.flyTo([3e-6, 4e-6, 0], 1e-5);
    expect(pointUnder(world, { width: 800, height: 600, x: 400, y: 300 })?.id).toBe(42);
    expect(pointUnder(world, { width: 800, height: 600, x: 10, y: 10 })).toBeUndefined();
  });

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
      extent: { centre: [0, 0, 0] as [number, number, number], radius: 1, core: { centre: [0, 0, 0] as [number, number, number], radius: 1 } },
    };
    const point = pointOf((name) => (name === "stars" ? domain : undefined), batch, 0);
    expect(point.id).toBe(42);
    expect(point.domain).toBe("stars");
    expect(point.distanceMpc).toBeCloseTo(5e-6, 10);
    expect(point.source).toBe(source.title);
    expect(point.kind).toBe("estimated");
  });
});
