// @vitest-environment node
import { describe, expect, it } from "vitest";

import { ExplorerManifest } from "../api/schemas.ts";
import { bytesOf, fixtureText, fixtureTiles, manifestOf } from "../test/fixtures.ts";
import { starColour } from "../scene/starColour.ts";
import { COLOUR_BANDS, MPC_PER, TileFailed, batchOf, tilesOf } from "./tiles.ts";

function domainOf(name: string) {
  const manifest = manifestOf(name);
  return { name, manifest, toMpc: MPC_PER[manifest.unit], tiles: tilesOf(name, manifest) };
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
      raw.points.forEach((point, i) => {
        for (const axis of [0, 1, 2] as const) {
          const drawn = tile.centre[axis] + (batch.offsets[i * 3 + axis] ?? 0);
          expect(Math.abs(drawn - point.position[axis] * domain.toMpc)).toBeLessThanOrEqual(step);
        }
        expect(batch.ids[i]).toBe(point.id);
      });
    }
  });

  it("gives each star the colour of its colour index, and every other point none of its own", async () => {
    for (const name of ["stars", "cosmic_web"]) {
      const domain = domainOf(name);
      const raw = fixtureTiles(name)[0];
      const tile = domain.tiles.find((t) => t.address === raw?.address);
      if (raw === undefined || tile === undefined) throw new Error("no fixture tile");
      const batch = await batchOf(domain, tile, bytesOf(raw.hex));
      raw.points.forEach((point, i) => {
        const rgba = [...batch.colours.slice(i * 4, i * 4 + 4)];
        if (name === "cosmic_web") {
          expect(rgba[3]).toBe(0);
          return;
        }
        const [index, bands] = (point as { colour?: [number, "B-V" | "BP-RP"] }).colour ?? [];
        if (index === undefined || bands === undefined) throw new Error("a fixture star without a colour index");
        expect(batch.colourIndices[i]).toBeCloseTo(index, 2);
        expect(COLOUR_BANDS[(batch.colourBands[i] ?? 0) - 1]).toBe(bands);
        expect(rgba).toEqual(starColour({ index: batch.colourIndices[i] ?? 0, bands }).map((c) => Math.round(c * 255)));
        expect(rgba[3]).toBeGreaterThan(0);
      });
    }
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
