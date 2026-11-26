// @vitest-environment node
// The Galaxy model in the explorer (see-the-galaxy-model): read with its definition, its version
// 5 tiles decoded into points drawn as light, counted apart from the objects, hidden where the
// objects that cover them are drawn, credited by the papers its definition cites, and never
// picked.

import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { type GalaxyModel, galaxyModel } from "../api/galaxyModel.ts";
import { NO_MASKS, masksOf } from "../scene/masks.ts";
import { COVERED_BY_STARS, COVERED_BY_TRACERS, MODEL_OPACITY, NOT_COVERED, batchOpacity } from "../scene/model.ts";
import { starBytes } from "../scene/starColour.ts";
import { bytesOf, fixtureText, fixtureTiles, manifestOf } from "../test/fixtures.ts";
import { hiddenInAddress } from "../ui/hidden.ts";
import { pickableOf } from "../ui/useWorld.ts";
import { Catalogue } from "./catalogue.ts";
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
  for (let i = 0; i < 2000 && drawn.length < tiles; i += 1) await new Promise((r) => setTimeout(r, 1));
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
    const look = lookOf(DEFINITION, MANIFEST);
    for (const p of DEFINITION.definition.populations) {
      // The young stars and knots of the arms by the Galaxy tracers, the others by the stars.
      expect(look.cover[p.code], p.id).toBe(p.density.form === "arms" ? COVERED_BY_TRACERS : COVERED_BY_STARS);
      expect([...look.colours.subarray(p.code * 4, p.code * 4 + 3)]).toEqual(p.colour.srgb);
    }
  });
});

describe("the Galaxy model's points", () => {
  it("are each in its population's colour, with what covers it, and of no kind, source or id", async () => {
    const batches = await modelBatches();
    const look = lookOf(DEFINITION, MANIFEST);
    const expected = fixtureTiles("model/simulated_model").flatMap((t) => t.points as unknown as { population: number; covered: boolean }[]);
    const points = batches.reduce((n, b) => n + b.covered!.length, 0);
    expect(points).toBe(expected.length);
    let covered = 0;
    for (const batch of batches) {
      const n = batch.covered!.length;
      expect([batch.kinds.length, batch.sources.length, batch.ids.length]).toEqual([0, 0, 0]);
      expect(batch.offsets.length).toBe(n * 3);
      for (let i = 0; i < n; i += 1) {
        const colour = [...(batch.colours?.subarray(i * 4, i * 4 + 4) ?? [])];
        const code = look.colours.findIndex((_, at) => at % 4 === 0 && at > 0 && colour.every((c, k) => look.colours[at + k] === c)) / 4;
        expect(code).toBeGreaterThan(0);
        if (batch.covered![i] !== NOT_COVERED) {
          expect(batch.covered![i]).toBe(look.cover[code]);
          covered += 1;
        }
      }
    }
    expect(covered).toBe(expected.filter((p) => p.covered).length);
  });

  it("are each as bright as the next, a share of their domain's opacity", () => {
    expect(batchOpacity(0.8, true)).toBeCloseTo(0.8 * MODEL_OPACITY);
    expect(batchOpacity(0.8, false)).toBe(0.8);
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
