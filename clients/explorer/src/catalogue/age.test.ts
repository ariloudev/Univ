// @vitest-environment node
// The age below which the Galaxy's tracers are drawn (choose-what-is-drawn): a tracer at or above
// it is neither drawn, nor picked, nor counted, and one without an age is drawn as its other
// groups say. The SIMULATED fixture domain, whose version 3 tiles carry ages, is served as the
// Galaxy's tracers.
import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { bytesOf, fixtureText, fixtureTiles } from "../test/fixtures.ts";
import type { Sight } from "../scene/frustum.ts";
import { Catalogue } from "./catalogue.ts";
import { type Batch, domainsOf } from "./tiles.ts";

const TRACERS = "galaxy_tracers";

/** Univ Server holding the SIMULATED fixture domain as the Galaxy's tracers. */
const http: Http = {
  fetch: (url) => {
    const path = url.split("?")[0] ?? "";
    if (path === "/v1/scale-domains") return Promise.resolve(Response.json({ release: 2, domains: [{ name: TRACERS }] }));
    if (path.endsWith("/manifest")) return Promise.resolve(new Response(fixtureText("simulated.manifest.json")));
    const address = path.split("/").at(-1);
    const tile = fixtureTiles("simulated").find((t) => t.address === address);
    return Promise.resolve(tile === undefined ? new Response("{}", { status: 404 }) : new Response(bytesOf(tile.hex)));
  },
  sleep: () => Promise.resolve(),
  now: () => 0,
};

const sight: Sight = {
  eye: [0, 0, 0],
  axes: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  focal: 1e9,
  inView: () => true,
};

/** Every tile of the tracers held, level of detail off, and the batches drawn. */
async function held() {
  const domains = await domainsOf(2, { http });
  const batches: Batch[] = [];
  const events = { onBatch: (b: Batch) => batches.push(b), onDrop: () => undefined, onChange: () => undefined };
  const catalogue = new Catalogue(2, domains, events, { http, keep: 0 });
  catalogue.setLevelOfDetail(false);
  catalogue.update(sight);
  const tiles = domains[0]?.tiles.length ?? 0;
  for (let i = 0; i < 2000 && batches.length < tiles; i += 1) await new Promise((r) => setTimeout(r, 1));
  expect(batches).toHaveLength(tiles);
  const counted = () => catalogue.tracerKinds.reduce((sum, n) => sum + n, 0);
  return { catalogue, batches, counted };
}

/** log10 of 30 million years. */
const BELOW_30_MYR = Math.log10(3e7);

describe("the Galaxy's tracers by age", () => {
  it("counts the tracers held and those of them that carry an age", async () => {
    const { catalogue, batches } = await held();
    const points = batches.reduce((n, b) => n + b.ids.length, 0);
    const aged = batches.reduce((n, b) => n + [...(b.ages ?? [])].filter((a) => !Number.isNaN(a)).length, 0);
    expect(aged).toBeGreaterThan(0);
    expect(catalogue.agesHeld()).toEqual({ tracers: points, aged });
  });

  it("draws, lets the pointer name and counts only the tracers younger than the age chosen, and those without an age", async () => {
    const { catalogue, batches, counted } = await held();
    catalogue.setAgeCut(BELOW_30_MYR);
    let kept = 0;
    let old = 0;
    for (const b of batches) {
      const drawn = catalogue.drawnPoints(b);
      const marked = catalogue.inside(b.tile.id);
      // A tile with no age keeps all its points.
      if (b.ages === undefined) kept += b.ids.length;
      b.ages?.forEach((age, i) => {
        // NaN, no age, is drawn.
        const young = !(age >= BELOW_30_MYR);
        expect(drawn(i)).toBe(young);
        // A tile none of whose points is cut keeps no mask: every point is drawn.
        expect(marked?.[i] ?? 1).toBe(young ? 1 : 0);
        if (young) kept += 1;
        else old += 1;
      });
    }
    expect(old).toBeGreaterThan(0);
    expect(counted()).toBe(kept);
    expect(catalogue.culled()).toBe(false);
  });

  it("draws every tracer again once any age is chosen", async () => {
    const { catalogue, batches, counted } = await held();
    const all = counted();
    catalogue.setAgeCut(BELOW_30_MYR);
    expect(counted()).toBeLessThan(all);
    catalogue.setAgeCut(undefined);
    expect(counted()).toBe(all);
    for (const b of batches) expect(catalogue.inside(b.tile.id)).toBeUndefined();
  });
});
