// @vitest-environment node
import { describe, expect, it } from "vitest";

import { ExplorerManifest } from "../api/schemas.ts";
import { bytesOf, fixtureText, fixtureTiles, manifestOf } from "../test/fixtures.ts";
import { starBytes } from "../scene/starColour.ts";
import { objectIndex, tracerBytes } from "../scene/tracers.ts";
import { shuffled } from "./shuffle.ts";
import { MPC_PER, TileFailed, batchOf, tilesOf } from "./tiles.ts";

function domainOf(name: string) {
  const manifest = manifestOf(name);
  return { name, manifest, toMpc: MPC_PER[manifest.unit], tiles: tilesOf(name, manifest) };
}

/** The index in `batch` of the point whose id is given. */
function indexById(batch: { ids: Float64Array }): (id: number) => number {
  const at = new Map([...batch.ids].map((id, i) => [id, i]));
  return (id) => {
    const i = at.get(id);
    if (i === undefined) throw new Error(`no point ${id}`);
    return i;
  };
}

describe("a tile", () => {
  it("is drawn at the ICRS positions its points give, in megaparsecs", async () => {
    for (const name of ["stars", "cosmic_web"]) {
      const domain = domainOf(name);
      const raw = fixtureTiles(name)[0];
      const tile = domain.tiles.find((t) => t.address === raw?.address);
      if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(domain, tile, bytesOf(raw.hex));
      const step = (domain.manifest.levels[0]?.step ?? 1) * domain.toMpc;
      expect(batch.ids.length).toBe(raw.points.length);
      const at = indexById(batch);
      raw.points.forEach((point) => {
        const i = at(point.id);
        for (const axis of [0, 1, 2] as const) {
          const drawn = tile.centre[axis] + (batch.offsets[i * 3 + axis] ?? 0);
          expect(Math.abs(drawn - point.position[axis] * domain.toMpc)).toBeLessThanOrEqual(step);
        }
      });
    }
  });

  it("holds its slice of its domain: from what the levels above keep to what its own level adds, or all that is left for a leaf", () => {
    // The simulated root keeps 8 of the domain's 60 points, two fifteenths; its children are leaves.
    const simulated = domainOf("simulated");
    const root = simulated.tiles.find((t) => t.parent === undefined);
    expect(root?.slice?.[0]).toBe(0);
    expect(root?.slice?.[1]).toBeCloseTo(2 / 15);
    for (const child of simulated.tiles.filter((t) => t.parent !== undefined)) {
      expect(child.slice?.[0]).toBeCloseTo(2 / 15);
      expect(child.slice?.[1]).toBe(1);
    }
    // A domain whose root is its only tile holds all of it.
    expect(domainOf("cosmic_web").tiles[0]?.slice).toEqual([0, 1]);
  });

  it("orders its points shuffled by its id, the same on every read", async () => {
    const domain = domainOf("cosmic_web");
    const raw = fixtureTiles("cosmic_web")[0];
    const tile = domain.tiles.find((t) => t.address === raw?.address);
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    const first = await batchOf(domain, tile, bytesOf(raw.hex));
    const again = await batchOf(domain, tile, bytesOf(raw.hex));
    expect([...first.ids]).toEqual(shuffled(raw.points.map((p) => p.id), tile.id));
    expect([...again.ids]).toEqual([...first.ids]);
  });

  it("gives each star the colour of its colour index, and every other point none of its own", async () => {
    for (const name of ["stars", "cosmic_web"]) {
      const domain = domainOf(name);
      const raw = fixtureTiles(name)[0];
      const tile = domain.tiles.find((t) => t.address === raw?.address);
      if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(domain, tile, bytesOf(raw.hex));
      if (name === "cosmic_web") {
        expect(batch.stars).toBeUndefined();
        continue;
      }
      const stars = batch.stars;
      if (stars === undefined) throw new Error("a star tile without its colours");
      const names = domain.manifest.encoding.colour_bands ?? [];
      expect(stars.coloured).toBe(raw.points.filter((p) => p.colour !== undefined).length);
      const at = indexById(batch);
      raw.points.forEach((point) => {
        const i = at(point.id);
        const [index, bands] = point.colour ?? [];
        if (index === undefined || bands === undefined) throw new Error("a fixture star without a colour index");
        expect((stars.hundredths[i] ?? 0) / 100).toBeCloseTo(index, 2);
        expect(names[(stars.bands[i] ?? 0) - 1]).toBe(bands);
        expect([...stars.bytes.slice(i * 4, i * 4 + 4)]).toEqual([...starBytes({ index: (stars.hundredths[i] ?? 0) / 100, bands })]);
      });
    }
  });

  it("draws a star whose preferred distance is inferred half as opaque as one of the same colour", async () => {
    const domain = domainOf("stars");
    const raw = fixtureTiles("stars")[0];
    const tile = domain.tiles.find((t) => t.address === raw?.address);
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    // The same tile with its first point's kind of value, the byte at 48 + 16n, made inferred.
    const bytes = bytesOf(raw.hex);
    const n = raw.points.length;
    bytes[48 + 16 * n] = domain.manifest.encoding.kinds.indexOf("inferred");
    const sha256 = Buffer.from(await crypto.subtle.digest("SHA-256", bytes)).toString("hex");
    const changed = { ...tile, node: { ...tile.node, sha256 } };
    const [before, after] = [await batchOf(domain, tile, bytesOf(raw.hex)), await batchOf(domain, changed, bytes)];
    const first = raw.points[0]?.id ?? -1;
    const [i, j] = [indexById(before)(first), indexById(after)(first)];
    const rgba = (batch: typeof before, k: number) => [...(batch.colours ?? []).slice(k * 4, k * 4 + 4)];
    const [kept, halved] = [rgba(before, i), rgba(after, j)];
    expect(halved.slice(0, 3)).toEqual(kept.slice(0, 3));
    expect(halved[3]).toBe(Math.round((kept[3] ?? 0) / 2));
    expect(before.kinds[i]).not.toBe(after.kinds[j]);
  });

  it("gives each point of the Galaxy's tracers its kind of object's colour, whichever source placed it", async () => {
    // The SIMULATED tracers domain's version 4 tiles: every ninth point of no kind.
    const domain = domainOf("simulated_tracers");
    const tracers = { ...domain, name: "galaxy_tracers" };
    let none = 0;
    const kinds = new Set<number>();
    for (const raw of fixtureTiles("simulated_tracers")) {
      const tile = domain.tiles.find((t) => t.address === raw.address);
      if (tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(tracers, tile, bytesOf(raw.hex));
      const objects = batch.objects ?? new Uint8Array();
      expect(objects.length).toBe(batch.ids.length);
      expect([...(batch.colours ?? [])]).toEqual([...tracerBytes(objects)]);
      const at = indexById(batch);
      for (const p of raw.points) expect(objects[at(p.id)]).toBe(objectIndex(p.object_kind ?? undefined));
      none += objects.filter((o) => o === 0).length;
      for (const o of objects) kinds.add(o);
    }
    expect(none).toBeGreaterThan(0);
    expect(kinds.size).toBeGreaterThan(2);
  });

  it("draws a tracer of a tile that gives no kind of object in the unknown kind's colour, and gives no other domain's points kinds", async () => {
    // A version 3 tile of another domain read as the tracers'.
    const domain = domainOf("cosmic_web");
    const tracers = { ...domain, name: "galaxy_tracers" };
    const raw = fixtureTiles("cosmic_web")[0];
    const tile = domain.tiles.find((t) => t.address === raw?.address);
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    const batch = await batchOf(tracers, tile, bytesOf(raw.hex));
    expect(batch.stars).toBeUndefined();
    expect([...(batch.objects ?? [])].every((o) => o === 0)).toBe(true);
    expect([...(batch.colours ?? []).slice(0, 4)]).toEqual([...tracerBytes(Uint8Array.of(0))]);
    const other = await batchOf(domain, tile, bytesOf(raw.hex));
    expect(other.colours).toBeUndefined();
    expect(other.objects).toBeUndefined();
  });

  it("gives each point of the Galaxy's tracers the logarithm of its age, or none, and no other domain's points any", async () => {
    // The SIMULATED domain carries ages in its version 3 tiles: read as the tracers'.
    const domain = domainOf("simulated");
    const tracers = { ...domain, name: "galaxy_tracers" };
    let aged = 0;
    for (const raw of fixtureTiles("simulated")) {
      const tile = domain.tiles.find((t) => t.address === raw.address);
      if (tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(tracers, tile, bytesOf(raw.hex));
      const at = indexById(batch);
      // A tile none of whose points carries an age has no ages at all.
      if (raw.points.every((p) => p.log_age === undefined)) expect(batch.ages).toBeUndefined();
      for (const p of raw.points) {
        const age = batch.ages?.[at(p.id)] ?? Number.NaN;
        if (p.log_age === undefined) expect(age).toBeNaN();
        else {
          expect(Math.abs((age ?? Number.NaN) - p.log_age)).toBeLessThanOrEqual(0.006);
          aged += 1;
        }
      }
      expect((await batchOf(domain, tile, bytesOf(raw.hex))).ages).toBeUndefined();
    }
    expect(aged).toBeGreaterThan(0);
  });

  it("keeps its stars' kinds of value where its manifest lists no colour bands", async () => {
    // A tile whose points carry no colour, read as a stars tile whose manifest names no bands.
    const domain = domainOf("cosmic_web");
    const { colour_bands: _, ...encoding } = domain.manifest.encoding;
    const old = { ...domain, name: "stars", manifest: { ...domain.manifest, encoding } };
    const raw = fixtureTiles("cosmic_web")[0];
    const tile = domain.tiles.find((t) => t.address === raw?.address);
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    expect((await batchOf(old, tile, bytesOf(raw.hex))).stars).toBeUndefined();
  });

  it("is read from a manifest in parsecs or megaparsecs, and no other unit", () => {
    expect(MPC_PER.pc).toBe(1e-6);
    expect(MPC_PER.Mpc).toBe(1);
    const manifest = JSON.parse(fixtureText("cosmic_web.manifest.json")) as Record<string, unknown>;
    expect(ExplorerManifest.safeParse(manifest).success).toBe(true);
    expect(ExplorerManifest.safeParse({ ...manifest, unit: "Mpc/h" }).success).toBe(false);
    expect(ExplorerManifest.safeParse({ ...manifest, rotations: undefined }).success).toBe(false);
  });

  it("whose bytes do not match its SHA-256 is refused", async () => {
    const domain = domainOf("cosmic_web");
    const raw = fixtureTiles("cosmic_web")[0];
    const tile = domain.tiles.find((t) => t.address === raw?.address);
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    const changed = bytesOf(raw.hex);
    changed[changed.length - 1] = (changed[changed.length - 1] ?? 0) ^ 1;
    await expect(batchOf(domain, tile, changed)).rejects.toBeInstanceOf(TileFailed);
  });
});
