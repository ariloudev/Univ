// @vitest-environment node
import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { DOMAINS, bytesOf, fixtureText, fixtureTiles } from "../test/fixtures.ts";
import { Catalogue, KEEP_ALLOWANCE } from "./catalogue.ts";
import { EVERYTHING } from "./lod.ts";
import { NO_MASKS, masksOf } from "../scene/masks.ts";
import { hiddenInAddress } from "../ui/hidden.ts";
import type { Sight } from "../scene/frustum.ts";
import type { Vec3 } from "../scene/vec.ts";
import { NOTHING, type Volume } from "../scene/volume.ts";
import { type Batch, domainsOf } from "./tiles.ts";

/** Univ Server's answer to `path` on the fixture release; `corrupt` (domain/address) altered,
 * `missing` (domain/address) answered 503. */
function answer(path: string, corrupt?: string, missing?: string, domains = DOMAINS): Response {
  if (path === "/v1/scale-domains") return Response.json({ release: 2, domains: domains.map((name) => ({ name })) });
  const manifest = /^\/v1\/scale-domains\/(\w+)\/manifest$/.exec(path);
  if (manifest !== null) return new Response(fixtureText(`${manifest[1]}.manifest.json`));
  const [, domain = "", address = ""] = /^\/v1\/scale-domains\/(\w+)\/tiles\/(\w+)$/.exec(path) ?? [];
  const tile = fixtureTiles(domain).find((t) => t.address === address);
  if (tile === undefined) return new Response("{}", { status: 404 });
  if (`${domain}/${address}` === missing) return Response.json({ status: 503, title: "Unavailable" }, { status: 503 });
  const bytes = bytesOf(tile.hex);
  if (`${domain}/${address}` === corrupt) bytes[0] = (bytes[0] ?? 0) ^ 0xff;
  return new Response(bytes);
}

/** Univ Server on the tile decoder's fixtures: the fixture release's three domains. */
function server(corrupt?: string, missing?: string, domains = DOMAINS): Http & { urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: (url) => {
      urls.push(url);
      return Promise.resolve(answer(url.split("?")[0] ?? "", corrupt, missing, domains));
    },
    sleep: () => Promise.resolve(),
    now: () => 0,
  };
}

/** Waits until `done` holds, failing after a second. */
async function until(done: () => boolean): Promise<void> {
  for (let i = 0; i < 1000; i += 1) {
    if (done()) return;
    await new Promise((r) => setTimeout(r, 1));
  }
  throw new Error("the catalogue did not settle");
}

/** A sight that sees every tile but those centred where `hidden` says, its points far apart. */
const sight = (hidden: (centre: Vec3) => boolean = () => false): Sight => ({
  eye: [0, 0, 0],
  axes: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  focal: 1e9,
  inView: (centre) => !hidden(centre),
});

/** Waits until `drawn` has not changed for 50 ms. */
async function steady(drawn: string[]): Promise<void> {
  let last = "";
  let still = 0;
  for (let i = 0; i < 2000 && still < 50; i += 1) {
    const now = drawn.join();
    still = now === last ? still + 1 : 0;
    last = now;
    await new Promise((r) => setTimeout(r, 1));
  }
}

async function loaded(http: Http, drawn: string[], budget?: number) {
  const domains = await domainsOf(2, { http });
  const events = {
    onBatch: (b: Batch) => drawn.push(b.tile.id),
    onDrop: (b: Batch) => drawn.splice(drawn.indexOf(b.tile.id), 1),
    onChange: () => undefined,
  };
  const catalogue = new Catalogue(2, domains, events, budget === undefined ? { http } : { http, budget });
  catalogue.update(sight());
  return catalogue;
}

/** `http`, which records the URLs it is asked for, with the answers for tiles whose address
 * is held back until released. */
function gated(http: Http & { urls: string[] }) {
  let held = new Set<string>();
  let open: () => void = () => undefined;
  let gate = Promise.resolve();
  const address = (url: string) => /\/tiles\/(\w+)/.exec(url.split("?")[0] ?? "")?.[1];
  return {
    http: {
      ...http,
      fetch: async (url: string) => {
        const at = address(url);
        if (at !== undefined && held.has(at)) await gate;
        return http.fetch(url);
      },
    },
    /** Holds back the answers for tiles `addresses` until `release`. */
    hold(addresses: string[]) {
      held = new Set(addresses);
      gate = new Promise((resolve) => (open = resolve));
    },
    release() {
      held = new Set();
      open();
    },
    /** How many times the tile `at` was read. */
    reads: (at: string) => http.urls.filter((url) => address(url) === at).length,
  };
}

/** The simulated domain on `http`: a root and eight children, four on each side of it in x;
 * room for the root and the larger side's children, and one more tile where `slack`; `keep`
 * points of dropped tiles kept drawn, by default all. */
async function twoSides(http: Http, keep = Number.POSITIVE_INFINITY, slack = false) {
  const tiles = (await domainsOf(2, { http }))[0]?.tiles ?? [];
  const root = tiles.find((t) => t.parent === undefined);
  if (root === undefined) throw new Error("no root");
  const side = (east: boolean) => tiles.filter((t) => t.parent !== undefined && t.centre[0] >= root.centre[0] === east);
  const points = (ts: typeof tiles) => ts.reduce((sum, t) => sum + t.points, 0);
  const largest = Math.max(...tiles.map((t) => t.points));
  const budget = root.points + Math.max(points(side(true)), points(side(false))) + (slack ? largest : 0);
  const drawn: string[] = [];
  const batches = new Map<string, Batch>();
  const events = {
    onBatch: (b: Batch) => {
      batches.set(b.tile.id, b);
      drawn.push(b.tile.id);
    },
    onDrop: (b: Batch) => drawn.splice(drawn.indexOf(b.tile.id), 1),
    onChange: () => undefined,
  };
  const catalogue = new Catalogue(2, await domainsOf(2, { http }), events, { http, budget, keep });
  const looking = (east: boolean) => {
    const away = new Set(side(!east).map((t) => t.id));
    return sight((centre) => tiles.some((t) => away.has(t.id) && t.centre[0] === centre[0] && t.centre[1] === centre[1] && t.centre[2] === centre[2]));
  };
  return { catalogue, drawn, batches, root, side, looking, budget };
}

describe("the catalogue", () => {
  it("draws every root tile, counts each kind of value, and credits the drawn domains' sources", async () => {
    const http = server();
    const drawn: string[] = [];
    const catalogue = await loaded(http, drawn);
    await until(() => drawn.length >= 3);
    expect(drawn.slice(0, 3).sort()).toEqual(["cosmic_web/r", "nearby_galaxies/r", "stars/r"]);
    expect(http.urls.filter((u) => u.includes("/tiles/")).every((u) => u.endsWith("release=2"))).toBe(true);
    expect(catalogue.kinds.reduce((a, b) => a + b, 0)).toBeGreaterThan(0);
    const credits = catalogue.credits();
    expect(credits.length).toBeGreaterThan(0);
    for (const credit of credits) {
      expect(credit.title).not.toBe("");
      expect(credit.citation).not.toBe("");
      expect(credit.credit).not.toBe("");
    }
  });

  it("leaves a tile that fails its SHA-256 undrawn and counts it", async () => {
    const drawn: string[] = [];
    const catalogue = await loaded(server("stars/r"), drawn);
    await until(() => catalogue.failed === 1 && drawn.length >= 2);
    expect(drawn).not.toContain("stars/r");
    expect(catalogue.unread).toBe(0);
  });

  it("drops the tiles the view leaves to make room for those it turns to, and counts their points out", async () => {
    const { catalogue, drawn, root, side, looking, budget } = await twoSides(server(undefined, undefined, ["simulated"]));
    const total = () => catalogue.kinds.reduce((a, b) => a + b, 0);
    catalogue.update(looking(true));
    await steady(drawn);
    expect([...drawn].sort()).toEqual([root.id, ...side(true).map((t) => t.id)].sort());
    expect(total()).toBeLessThanOrEqual(budget);
    // Turned to the other side: its children replace the first side's, and the root stays.
    catalogue.update(looking(false));
    await steady(drawn);
    expect([...drawn].sort()).toEqual([root.id, ...side(false).map((t) => t.id)].sort());
    expect(total()).toBeLessThanOrEqual(budget);
  });

  it("keeps the tiles it drops drawn and counted until the tiles fetched in their place are drawn", async () => {
    const gate = gated(server(undefined, undefined, ["simulated"]));
    const { catalogue, drawn, root, side, looking } = await twoSides(gate.http);
    const total = () => catalogue.kinds.reduce((a, b) => a + b, 0);
    catalogue.update(looking(true));
    await steady(drawn);
    const before = total();
    // Turned to the other side, whose tiles are held back: the first side stays drawn, counted.
    gate.hold(side(false).map((t) => t.address));
    catalogue.update(looking(false));
    await steady(drawn);
    expect([...drawn].sort()).toEqual([root.id, ...side(true).map((t) => t.id)].sort());
    expect(total()).toBe(before);
    // Once they are drawn, the first side is let go.
    gate.release();
    await steady(drawn);
    expect([...drawn].sort()).toEqual([root.id, ...side(false).map((t) => t.id)].sort());
  });

  it("keeps no dropped tile drawn beyond its allowance", async () => {
    expect(KEEP_ALLOWANCE).toBe(1_500_000);
    // An allowance of none: the first side is let go as soon as it is dropped.
    const gate = gated(server(undefined, undefined, ["simulated"]));
    const sides = await twoSides(gate.http, 0);
    const { catalogue, drawn, root, side, looking } = sides;
    catalogue.update(looking(true));
    await steady(drawn);
    gate.hold(side(false).map((t) => t.address));
    catalogue.update(looking(false));
    await steady(drawn);
    expect(drawn).toEqual([root.id]);
    gate.release();
    await steady(drawn);
  });

  it("keeps the dropped tiles that fit its allowance drawn, and those beyond it no more", async () => {
    const gate = gated(server(undefined, undefined, ["simulated"]));
    const first = await twoSides(gate.http);
    const keep = Math.max(...first.side(true).map((t) => t.points));
    const { catalogue, drawn, root, side, looking } = await twoSides(gate.http, keep);
    catalogue.update(looking(true));
    await steady(drawn);
    gate.hold(side(false).map((t) => t.address));
    catalogue.update(looking(false));
    await steady(drawn);
    // Some of the first side stay drawn while the second's tiles are held back, within the
    // allowance; the rest are drawn no more.
    const kept = side(true).filter((t) => drawn.includes(t.id));
    expect(kept.length).toBeGreaterThan(0);
    expect(kept.length).toBeLessThan(side(true).length);
    expect(kept.reduce((sum, t) => sum + t.points, 0)).toBeLessThanOrEqual(keep);
    expect(drawn).toContain(root.id);
    gate.release();
    await steady(drawn);
  });

  it("draws on a kept tile the view wants again without reading it again", async () => {
    // Room for one more tile: turned back while one tile of the second side is held back, the
    // view drops the second side's drawn tiles and wants the first side's again, still kept.
    const gate = gated(server(undefined, undefined, ["simulated"]));
    const { catalogue, drawn, root, side, looking } = await twoSides(gate.http, Number.POSITIVE_INFINITY, true);
    catalogue.update(looking(true));
    await steady(drawn);
    const [late] = side(false);
    if (late === undefined) throw new Error("no tile");
    gate.hold([late.address]);
    catalogue.update(looking(false));
    await steady(drawn);
    catalogue.update(looking(true));
    await steady(drawn);
    expect(drawn).toEqual(expect.arrayContaining([root.id, ...side(true).map((t) => t.id)]));
    for (const t of side(true)) expect(gate.reads(t.address)).toBe(1);
    gate.release();
    await steady(drawn);
  });

  it("draws every tile with level of detail off, whatever the view and the budget, and comes back within the budget once it is on", async () => {
    const { catalogue, drawn, root, side, looking, budget } = await twoSides(server(undefined, undefined, ["simulated"]), 0);
    const total = () => catalogue.kinds.reduce((a, b) => a + b, 0);
    catalogue.update(looking(true));
    await steady(drawn);
    catalogue.setLevelOfDetail(false);
    await steady(drawn);
    const every = [root, ...side(true), ...side(false)].map((t) => t.id);
    expect([...drawn].sort()).toEqual(every.sort());
    expect(total()).toBeGreaterThan(budget);
    catalogue.setLevelOfDetail(true);
    await steady(drawn);
    expect([...drawn].sort()).toEqual([root.id, ...side(true).map((t) => t.id)].sort());
    expect(total()).toBeLessThanOrEqual(budget);
  });

  it("draws every point of a root, of other tiles those whose rank lies below the share their distance needs, and every one with level of detail off", async () => {
    const { catalogue, drawn, batches, root, side, looking } = await twoSides(server(undefined, undefined, ["simulated"]));
    catalogue.update(looking(true));
    await steady(drawn);
    const [child] = side(true);
    if (child === undefined) throw new Error("no child");
    expect(catalogue.share(root.id)).toBe(1);
    expect(catalogue.share(child.id)).toBe(1);
    const childBatch = batches.get(child.id);
    const rootBatch = batches.get(root.id);
    if (childBatch === undefined || rootBatch === undefined) throw new Error("not drawn");
    const every = (b: Batch, drawnPoint: (i: number) => boolean) => Array.from({ length: b.ids.length }, (_, i) => drawnPoint(i));
    expect(every(childBatch, catalogue.drawnPoints(childBatch))).not.toContain(false);
    // Seen from so far that the domain needs no more than its root holds, 8 of its 60 points.
    catalogue.update({ ...looking(true), focal: 1e-9 });
    expect(catalogue.share(root.id)).toBe(1);
    expect(catalogue.share(child.id)).toBeCloseTo(0, 6);
    expect(every(rootBatch, catalogue.drawnPoints(rootBatch))).not.toContain(false);
    expect(every(childBatch, catalogue.drawnPoints(childBatch))).not.toContain(true);
    const detail = catalogue.detail("simulated");
    expect(detail.cumulative[0]).toBeCloseTo(8 / 60);
    expect(detail.cumulative[1]).toBe(1);
    expect(detail.onsets[1]).toBeLessThan(1e-6);
    catalogue.setLevelOfDetail(false);
    expect(catalogue.share(child.id)).toBe(1);
    expect(every(childBatch, catalogue.drawnPoints(childBatch))).not.toContain(false);
    expect(catalogue.detail("simulated")).toBe(EVERYTHING);
    await steady(drawn);
  });

  it("drops what the view does not want once level of detail is on again, though every fetch is still on its way", async () => {
    // Level of detail off, on a budget of the root alone: the root and r0 and r1 arrive, and the
    // six other children are held back, so that every fetch is on its way.
    const gate = gated(server(undefined, undefined, ["simulated"]));
    gate.hold(["r2", "r3", "r4", "r5", "r6", "r7"]);
    const domains = await domainsOf(2, { http: gate.http });
    const root = domains[0]?.tiles.find((t) => t.parent === undefined);
    if (root === undefined) throw new Error("no root");
    const drawn: string[] = [];
    const events = {
      onBatch: (b: Batch) => drawn.push(b.tile.id),
      onDrop: (b: Batch) => drawn.splice(drawn.indexOf(b.tile.id), 1),
      onChange: () => undefined,
    };
    const catalogue = new Catalogue(2, domains, events, { http: gate.http, budget: root.points, keep: 0 });
    catalogue.setLevelOfDetail(false);
    catalogue.update(sight());
    await until(() => drawn.length === 3);
    await steady(drawn);
    expect([...drawn].sort()).toEqual(["simulated/r", "simulated/r0", "simulated/r1"]);
    catalogue.setLevelOfDetail(true);
    expect(drawn).toEqual(["simulated/r"]);
    gate.release();
    await steady(drawn);
  });

  describe("culled to a structure's volume", () => {
    /** The simulated domain with level of detail off, culled where `culled` from the start, every
     * tile it wants drawn; and the box east of its root's centre, which its eastern children's
     * cubes meet and its western children's do not. */
    async function culled(culledFirst: boolean, levelOfDetail = false) {
      const http = server(undefined, undefined, ["simulated"]);
      const domains = await domainsOf(2, { http });
      const root = domains[0]?.tiles.find((t) => t.parent === undefined);
      if (root === undefined) throw new Error("no root");
      const big = root.edge * 10;
      const east: Volume = {
        kind: "box",
        approximate: true,
        bounds: { min: [root.centre[0] + root.edge / 100, -big, -big], max: [big, big, big] },
      };
      const drawn: string[] = [];
      const batches = new Map<string, Batch>();
      const events = {
        onBatch: (b: Batch) => {
          batches.set(b.tile.id, b);
          drawn.push(b.tile.id);
        },
        onDrop: (b: Batch) => drawn.splice(drawn.indexOf(b.tile.id), 1),
        onChange: () => undefined,
      };
      const catalogue = new Catalogue(2, domains, events, { http, keep: 0 });
      catalogue.setLevelOfDetail(levelOfDetail);
      if (culledFirst) catalogue.setCull(east);
      catalogue.update(sight());
      await steady(drawn);
      const eastern = domains[0]?.tiles.filter((t) => t.parent !== undefined && t.centre[0] > root.centre[0]).map((t) => t.id) ?? [];
      const inEast = (b: Batch, i: number) => b.tile.centre[0] + (b.offsets[3 * i] ?? 0) >= east.bounds.min[0];
      const counted = () => catalogue.kinds.reduce((sum, n) => sum + n, 0);
      return { catalogue, drawn, batches, root, east, eastern, inEast, counted };
    }

    it("fetches only the tiles whose cubes meet the box bounding the volume, with level of detail on or off", async () => {
      for (const levelOfDetail of [false, true]) {
        const { drawn, root, eastern } = await culled(true, levelOfDetail);
        expect([...drawn].sort()).toEqual([root.id, ...eastern].sort());
      }
    });

    it("fetches no tile, a root neither, whose cube misses the box, nor any while culled to nothing", async () => {
      for (const levelOfDetail of [false, true]) {
        const http = server(undefined, undefined, ["simulated"]);
        const domains = await domainsOf(2, { http });
        const root = domains[0]?.tiles.find((t) => t.parent === undefined);
        if (root === undefined) throw new Error("no root");
        const far = root.centre[0] + root.edge * 10;
        const away: Volume = { kind: "box", approximate: true, bounds: { min: [far, far, far], max: [far + 1, far + 1, far + 1] } };
        for (const volume of [away, NOTHING]) {
          const drawn: string[] = [];
          const events = { onBatch: (b: Batch) => drawn.push(b.tile.id), onDrop: () => undefined, onChange: () => undefined };
          const catalogue = new Catalogue(2, domains, events, { http, keep: 0 });
          catalogue.setLevelOfDetail(levelOfDetail);
          catalogue.setCull(volume);
          catalogue.update(sight());
          await steady(drawn);
          expect(drawn).toEqual([]);
        }
      }
    });

    it("draws none of a tile with no point inside, and counts none", async () => {
      const { catalogue, drawn, root, east, eastern } = await culled(false);
      catalogue.setCull(east);
      await steady(drawn);
      const western = drawn.filter((id) => id !== root.id && !eastern.includes(id));
      expect(western).toEqual([]);
      // Culled to a sliver in its cube's corner, which no point reaches, the root is held but
      // draws nothing.
      const corner = root.centre.map((c) => c + root.edge * 0.499) as unknown as Vec3;
      const sliver: Volume = { kind: "box", approximate: true, bounds: { min: corner, max: corner } };
      catalogue.setCull(sliver);
      expect(catalogue.inside(root.id)).toBeDefined();
      expect(catalogue.share(root.id)).toBe(0);
      expect(catalogue.kinds.reduce((sum, n) => sum + n, 0)).toBe(0);
    });

    it("drops the tiles held whose cubes miss it, and fetches them again once no longer culled", async () => {
      const { catalogue, drawn, root, east, eastern } = await culled(false);
      expect(drawn).toHaveLength(9);
      catalogue.setCull(east);
      await steady(drawn);
      expect([...drawn].sort()).toEqual([root.id, ...eastern].sort());
      catalogue.setCull(undefined);
      await steady(drawn);
      expect(drawn).toHaveLength(9);
    });

    it("counts in the key only the points inside, and every point once no longer culled", async () => {
      const { catalogue, drawn, batches, east, inEast, counted } = await culled(false);
      const all = counted();
      catalogue.setCull(east);
      await steady(drawn);
      let inside = 0;
      for (const id of drawn) {
        const b = batches.get(id);
        for (let i = 0; b !== undefined && i < b.kinds.length; i += 1) if (inEast(b, i)) inside += 1;
      }
      expect(inside).toBeGreaterThan(0);
      expect(counted()).toBe(inside);
      catalogue.setCull(undefined);
      await steady(drawn);
      expect(counted()).toBe(all);
    });

    it("draws, and so lets the pointer name, only the points inside", async () => {
      const { catalogue, batches, root, east, inEast } = await culled(false);
      catalogue.setCull(east);
      const batch = batches.get(root.id);
      if (batch === undefined) throw new Error("no root drawn");
      const drawnPoint = catalogue.drawnPoints(batch);
      const marked = catalogue.inside(root.id);
      for (let i = 0; i < batch.kinds.length; i += 1) {
        expect(drawnPoint(i)).toBe(inEast(batch, i));
        expect(marked?.[i]).toBe(inEast(batch, i) ? 1 : 0);
      }
      catalogue.setCull(undefined);
      expect(catalogue.inside(root.id)).toBeUndefined();
      expect(catalogue.drawnPoints(batch)(0)).toBe(true);
    });
  });

  it("does not count a tile Univ Server could not serve as failing its check", async () => {
    const drawn: string[] = [];
    const catalogue = await loaded(server(undefined, "stars/r"), drawn);
    await until(() => drawn.length >= 2);
    expect(catalogue.failed).toBe(0);
    expect(drawn).not.toContain("stars/r");
  });

  it("counts and credits only the points that what is turned off leaves drawn, and all again once shown", async () => {
    const drawn: string[] = [];
    const catalogue = await loaded(server(), drawn);
    await until(() => drawn.length >= 3);
    await steady(drawn);
    const total = () => catalogue.kinds.reduce((a, b) => a + b, 0);
    const before = { kinds: total(), stars: catalogue.stars.coloured + catalogue.stars.grey, credits: catalogue.credits().length };
    expect(before.stars).toBeGreaterThan(0);
    const domains = await domainsOf(2, { http: server() });
    catalogue.setMasks(masksOf(hiddenInAddress("?hide=domain:stars"), domains));
    expect(catalogue.stars.coloured + catalogue.stars.grey).toBe(0);
    expect(total()).toBe(before.kinds);
    expect(catalogue.credits().every((c) => c.domain !== "stars")).toBe(true);
    catalogue.setMasks(masksOf(hiddenInAddress("?hide=kind:measured,kind:estimated,kind:inferred,kind:reconstructed"), domains));
    expect(total()).toBe(0);
    expect(catalogue.credits()).toEqual([]);
    const everySource = domains.flatMap((d) => d.manifest.sources.map((s) => `source:${s.id}`)).join(",");
    catalogue.setMasks(masksOf(hiddenInAddress(`?hide=${everySource}`), domains));
    expect({ kinds: total(), stars: catalogue.stars.coloured + catalogue.stars.grey }).toEqual({ kinds: 0, stars: 0 });
    catalogue.setMasks(NO_MASKS);
    expect({ kinds: total(), stars: catalogue.stars.coloured + catalogue.stars.grey, credits: catalogue.credits().length }).toEqual(before);
  });
});
