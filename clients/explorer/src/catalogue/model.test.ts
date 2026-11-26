// @vitest-environment node
// The Galaxy model in the explorer (see-the-galaxy-model): read with its definition, its version
// 5 tiles decoded into points drawn as light, counted apart from the objects, hidden where the
// objects that cover them are drawn, credited by the papers its definition cites, and never
// picked.

import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { type GalaxyModel, galaxyModel } from "../api/galaxyModel.ts";
import { NO_MASKS, masksOf } from "../scene/masks.ts";
import { ARM_BRIGHTNESS, COVERED_BY_STARS, COVERED_BY_TRACERS, MODEL_OPACITY, NOT_COVERED, batchOpacity, coverOf } from "../scene/model.ts";
import { starBytes } from "../scene/starColour.ts";
import { bytesOf, fixtureText, fixtureTiles, manifestOf } from "../test/fixtures.ts";
import { hiddenInAddress } from "../ui/hidden.ts";
import { pickableOf } from "../ui/useWorld.ts";
import { Catalogue } from "./catalogue.ts";
import { EVERYTHING } from "./lod.ts";
import { type Batch, batchOf, domainsOf, lookOf } from "./tiles.ts";

/** The repository's definition as Univ Server answers for it. */
const DEFINITION = JSON.parse(fixtureText("model/galaxy_model.json")) as GalaxyModel;

/** The manifest of the fixture's model tiles. */
const MANIFEST = manifestOf("model/simulated_model");

/** The fixture of each domain the test server serves. */
const FIXTURES: Record<string, string> = { stars: "stars", galaxy_model: "model/simulated_model" };

/** Univ Server on the fixtures: the stars and the Galaxy model, its definition where `model`. */
function server(model = true): Http & { urls: string[] } {
  const urls: string[] = [];
  const answer = (path: string): Response => {
    if (path === "/v1/scale-domains") return Response.json({ release: 2, domains: Object.keys(FIXTURES).map((name) => ({ name })) });
    if (path === "/v1/galaxy-model") {
      return model ? Response.json(DEFINITION) : Response.json({ status: 404, title: "No Galaxy model" }, { status: 404 });
    }
    const manifest = /^\/v1\/scale-domains\/(\w+)\/manifest$/.exec(path);
    if (manifest !== null) return new Response(fixtureText(`${FIXTURES[manifest[1] ?? ""]}.manifest.json`));
    const [, domain = "", address = ""] = /^\/v1\/scale-domains\/(\w+)\/tiles\/(\w+)$/.exec(path) ?? [];
    const tile = fixtureTiles(FIXTURES[domain] ?? "").find((t) => t.address === address);
    return tile === undefined ? new Response("{}", { status: 404 }) : new Response(bytesOf(tile.hex));
  };
  return {
    urls,
    fetch: (url) => {
      urls.push(url);
      return Promise.resolve(answer(url.split("?")[0] ?? ""));
    },
    sleep: () => Promise.resolve(),
    now: () => 0,
  };
}

/** The population code and cover of each point `batch` draws, sorted: its code read back from
 * its colour by `look`. */
function drawnOf(batch: Batch, look: ReturnType<typeof lookOf>): string[] {
  const words = new Uint32Array(batch.colours?.buffer ?? new ArrayBuffer(0));
  const byCode = new Uint32Array(look.colours.buffer);
  return [...(batch.covered ?? [])].map((cover, i) => `${byCode.indexOf(words[i] ?? 0, 1)}/${cover}`).sort();
}

/** The population code and cover of each of a fixture tile's `points`, sorted, as its manifest
 * names what covers each population. */
function wantedOf(points: readonly { population?: number; covered?: boolean }[]): string[] {
  const coverBy = new Map((MANIFEST.encoding.populations ?? []).map((e) => [e.code, coverOf(e.covered_by)]));
  return points.map((p) => `${p.population ?? 0}/${p.covered === true ? (coverBy.get(p.population ?? 0) ?? 0) : NOT_COVERED}`).sort();
}

/** Every model tile of the fixture as batches. */
async function modelBatches(): Promise<Batch[]> {
  const domains = await domainsOf(2, { http: server() });
  const model = domains.find((d) => d.name === "galaxy_model");
  if (model === undefined) throw new Error("no model domain");
  return Promise.all(model.tiles.map((t) => batchOf(model, t, bytesOf(fixtureTiles("model/simulated_model").find((f) => f.address === t.address)?.hex ?? ""))));
}

/** A catalogue of the fixture's stars and model, every tile drawn. */
async function catalogueOf(): Promise<Catalogue> {
  const http = server();
  const domains = await domainsOf(2, { http });
  const drawn: string[] = [];
  const events = { onBatch: (b: Batch) => drawn.push(b.tile.id), onDrop: () => undefined, onChange: () => undefined };
  const catalogue = new Catalogue(2, domains, events, { http });
  const tiles = domains.reduce((n, d) => n + d.tiles.length, 0);
  catalogue.update({ eye: [0, 0, 0], axes: [[1, 0, 0], [0, 1, 0], [0, 0, 1]], focal: 1e9, inView: () => true });
  for (let i = 0; drawn.length < tiles; i += 1) {
    if (i === 2000) throw new Error("the catalogue did not settle");
    await new Promise((r) => setTimeout(r, 1));
  }
  return catalogue;
}

describe("the Galaxy model's definition", () => {
  it("gives each derived display colour as the explorer colours a star of its B−V", () => {
    let derived = 0;
    for (const p of DEFINITION.definition.populations) {
      if (p.colour.srgb_from.from !== "derived") continue;
      const bv = p.colour.b_minus_v?.value;
      expect(bv, p.id).toBeDefined();
      const [r, g, b] = starBytes({ index: bv ?? 0, bands: "B-V" });
      expect([r, g, b], p.id).toEqual(p.colour.srgb);
      derived += 1;
    }
    expect(derived).toBeGreaterThan(0);
  });

  it("is read once for a release, and is none where the release holds none", async () => {
    expect((await galaxyModel(7, { http: server() }))?.domain).toBe("galaxy_model");
    expect(await galaxyModel(8, { http: server(false) })).toBeUndefined();
    const without = await domainsOf(9, { http: server(false) });
    expect(without.map((d) => d.name)).toEqual(["stars"]);
  });

  it("left out where it cannot be read, the release's objects drawn without it", async () => {
    const http = server();
    const failing = { ...http, fetch: (url: string) => (url.includes("/v1/galaxy-model") ? Promise.resolve(Response.json({ status: 500 }, { status: 500 })) : http.fetch(url)) };
    const domains = await domainsOf(10, { http: failing });
    expect(domains.map((d) => d.name)).toEqual(["stars"]);
  });

  it("covers each population's points by the objects of the domain its manifest entry names", () => {
    const look = lookOf(DEFINITION);
    for (const p of DEFINITION.definition.populations) {
      // The young stars and knots of the arms by the Galaxy tracers, the others by the stars.
      const entry = MANIFEST.encoding.populations?.find((e) => e.code === p.code);
      expect(coverOf(entry?.covered_by), p.id).toBe(p.density.form === "arms" ? COVERED_BY_TRACERS : COVERED_BY_STARS);
      expect([...look.colours.subarray(p.code * 4, p.code * 4 + 3)]).toEqual(p.colour.srgb);
    }
    expect(coverOf(undefined)).toBe(NOT_COVERED);
  });
});

describe("the Galaxy model's points", () => {
  it("are each in its population's colour, with what covers it, and of no kind, source or id", async () => {
    const batches = await modelBatches();
    const look = lookOf(DEFINITION);
    const fixture = new Map(fixtureTiles("model/simulated_model").map((t) => [t.address, t.points]));
    for (const batch of batches) {
      expect([batch.kinds.length, batch.sources.length, batch.ids.length]).toEqual([0, 0, 0]);
      expect(batch.offsets.length).toBe((batch.covered?.length ?? 0) * 3);
      // Shuffled: the tile's colours and covers, counted, are the fixture's.
      expect(drawnOf(batch, look)).toEqual(wantedOf(fixture.get(batch.tile.address) ?? []));
    }
  });

  it("are each as bright as the next, a share of their domain's opacity", () => {
    expect(batchOpacity(0.8, true)).toBeCloseTo(0.8 * MODEL_OPACITY);
    expect(batchOpacity(0.8, false)).toBe(0.8);
  });

  it("are drawn at their population's brightness, the arms' young stars and knots 6 times the rest", () => {
    const look = lookOf(DEFINITION);
    const alpha = (code: number) => look.colours[code * 4 + 3] ?? 0;
    const arms = DEFINITION.definition.populations.filter((p) => p.density.form === "arms");
    const rest = DEFINITION.definition.populations.filter((p) => p.density.form !== "arms");
    expect(arms.map((p) => p.id)).toEqual(["young_stars", "star_forming_knots"]);
    for (const p of arms) expect(alpha(p.code), p.id).toBe(255);
    for (const p of rest) expect(alpha(p.code) * ARM_BRIGHTNESS, p.id).toBeCloseTo(255, -1);
  });

  it("are no object the pointer names or a click chooses", async () => {
    const [batch] = await modelBatches();
    expect(pickableOf(batch!, NO_MASKS)).toBeUndefined();
  });

  it("hide the covered ones while the domain of their counterparts is drawn", () => {
    const held = ["stars", "galaxy_tracers", "galaxy_model"].map((name) => ({ name, manifest: { sources: [] } }));
    const hidden = (address: string, domains = held) => masksOf(hiddenInAddress(address), domains).covered;
    expect(hidden("")).toBe((1 << COVERED_BY_STARS) | (1 << COVERED_BY_TRACERS));
    expect(hidden("?hide=domain:stars")).toBe(1 << COVERED_BY_TRACERS);
    expect(hidden("?hide=domain:galaxy_tracers")).toBe(1 << COVERED_BY_STARS);
    expect(hidden("?hide=domain:stars,domain:galaxy_tracers")).toBe(0);
    // A release without the Galaxy tracers has them off: their counterparts' points are drawn.
    expect(hidden("", held.slice(0, 1))).toBe(1 << COVERED_BY_STARS);
  });
});

describe("the catalogue with the Galaxy model", () => {
  it("counts the model's points apart from the objects, whatever the switches of kinds and sources", async () => {
    const domains = await domainsOf(2, { http: server() });
    const catalogue = await catalogueOf();
    catalogue.setMasks(masksOf(hiddenInAddress(""), domains));
    const objects = () => catalogue.kinds.reduce((a, b) => a + b, 0) + catalogue.stars.coloured + catalogue.stars.grey;
    const batches = await modelBatches();
    const all = batches.reduce((n, b) => n + b.covered!.length, 0);
    const uncovered = batches.reduce((n, b) => n + b.covered!.filter((c) => c === NOT_COVERED || c === COVERED_BY_TRACERS).length, 0);
    // The stars are drawn: the model points they cover are not, nor counted.
    expect(catalogue.modelPoints).toBe(uncovered);
    const stars = objects();
    catalogue.setMasks(masksOf(hiddenInAddress("?hide=kind:measured,kind:estimated,kind:inferred,kind:reconstructed"), domains));
    expect(objects()).toBe(0);
    expect(catalogue.modelPoints).toBe(uncovered);
    // The stars off: every model point is drawn.
    catalogue.setMasks(masksOf(hiddenInAddress("?hide=domain:stars"), domains));
    expect(catalogue.modelPoints).toBe(all);
    expect(stars).toBeGreaterThan(0);
  });

  it("credits the papers of the model's definition while its domain is drawn", async () => {
    const domains = await domainsOf(2, { http: server() });
    const catalogue = await catalogueOf();
    catalogue.setMasks(masksOf(hiddenInAddress(""), domains));
    expect(catalogue.model()?.papers.map((p) => p.id)).toEqual(DEFINITION.definition.papers.map((p) => p.id));
    expect(catalogue.credits().some((c) => c.domain === "galaxy_model")).toBe(false);
    catalogue.setMasks(masksOf(hiddenInAddress("?hide=domain:galaxy_model"), domains));
    expect(catalogue.model()).toBeUndefined();
    expect(catalogue.modelPoints).toBe(0);
  });
});

describe("the Galaxy model with level of detail off", () => {
  it("is still held and drawn by level of detail, within the budget, while every tile of objects is fetched", async () => {
    const http = server();
    const domains = await domainsOf(2, { http });
    const tilesOf = (name: string) => domains.find((d) => d.name === name)?.tiles ?? [];
    const drawn: string[] = [];
    const events = { onBatch: (b: Batch) => drawn.push(b.tile.id), onDrop: (b: Batch) => drawn.splice(drawn.indexOf(b.tile.id), 1), onChange: () => undefined };
    // A budget of the model's root alone: level of detail on would hold no more of anything.
    const root = tilesOf("galaxy_model").find((t) => t.parent === undefined);
    const catalogue = new Catalogue(2, domains, events, { http, budget: root?.points ?? 0 });
    catalogue.setLevelOfDetail(false);
    catalogue.update({ eye: [0, 0, 0], axes: [[1, 0, 0], [0, 1, 0], [0, 0, 1]], focal: 1e3, inView: () => true });
    const stars = tilesOf("stars").length;
    for (let i = 0; drawn.filter((id) => id.startsWith("stars/")).length < stars; i += 1) {
      if (i === 2000) throw new Error("the stars' tiles did not all arrive");
      await new Promise((r) => setTimeout(r, 1));
    }
    await new Promise((r) => setTimeout(r, 50));
    const models = drawn.filter((id) => id.startsWith("galaxy_model/"));
    expect(models.length).toBeGreaterThan(0);
    expect(models.length).toBeLessThan(tilesOf("galaxy_model").length);
    expect(catalogue.detail("galaxy_model")).not.toBe(EVERYTHING);
    expect(catalogue.detail("stars")).toBe(EVERYTHING);
  });
});
