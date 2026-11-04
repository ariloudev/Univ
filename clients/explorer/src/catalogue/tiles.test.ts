// @vitest-environment node
import { readFileSync } from "node:fs";

import type { TileManifest } from "univ-tile-decoder";
import { describe, expect, it } from "vitest";

import { TileFailed, batchOf, megaparsecsPer, tilesOf } from "./tiles.ts";

const FIXTURES = new URL("../../../tile-decoder/fixtures/", import.meta.url);

function fixture(domain: string) {
  const manifest = JSON.parse(readFileSync(new URL(`${domain}.manifest.json`, FIXTURES), "utf8")) as TileManifest;
  const tiles = JSON.parse(readFileSync(new URL(`${domain}.tiles.json`, FIXTURES), "utf8")) as {
    tiles: { address: string; hex: string; points: { position: number[]; id: number; kind: string }[] }[];
  };
  const toMpc = megaparsecsPer(manifest.unit);
  return { name: domain, manifest, toMpc, tiles: tilesOf(domain, manifest, toMpc), raw: tiles.tiles };
}

const bytes = (hex: string) => Uint8Array.from(hex.match(/../g) ?? [], (b) => Number.parseInt(b, 16));

describe("a tile", () => {
  it("is drawn at the ICRS positions its points give, in megaparsecs", async () => {
    for (const name of ["stars", "cosmic_web"]) {
      const domain = fixture(name);
      const raw = domain.raw[0];
      const tile = domain.tiles.find((t) => t.address === raw?.address);
      if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(domain, tile, bytes(raw.hex));
      expect(batch.ids.length).toBe(raw.points.length);
      raw.points.forEach((point, i) => {
        const step = (domain.manifest.levels[0]?.step ?? 1) * domain.toMpc;
        for (let axis = 0; axis < 3; axis += 1) {
          const drawn = batch.centre[axis as 0 | 1 | 2] + (batch.offsets[i * 3 + axis] ?? 0);
          expect(Math.abs(drawn - (point.position[axis] ?? 0) * domain.toMpc)).toBeLessThanOrEqual(step);
        }
        expect(batch.ids[i]).toBe(point.id);
      });
    }
  });

  it("converts parsecs and megaparsecs to megaparsecs", () => {
    expect(megaparsecsPer("pc")).toBe(1e-6);
    expect(megaparsecsPer("Mpc")).toBe(1);
    expect(() => megaparsecsPer("Mpc/h")).toThrow();
  });

  it("whose bytes do not match its SHA-256 is refused", async () => {
    const domain = fixture("cosmic_web");
    const raw = domain.raw[0];
    const tile = domain.tiles[0];
    if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
    const changed = bytes(raw.hex);
    changed[changed.length - 1] = (changed[changed.length - 1] ?? 0) ^ 1;
    await expect(batchOf(domain, tile, changed)).rejects.toBeInstanceOf(TileFailed);
  });
});
