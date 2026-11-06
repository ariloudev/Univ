// @vitest-environment node
import { describe, expect, it } from "vitest";

import { ExplorerManifest } from "../api/schemas.ts";
import { bytesOf, fixtureText, fixtureTiles, manifestOf } from "../test/fixtures.ts";
import { starBytes } from "../scene/starColour.ts";
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
