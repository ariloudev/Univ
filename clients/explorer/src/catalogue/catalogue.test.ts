// @vitest-environment node
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { Catalogue } from "./catalogue.ts";
import { domainsOf } from "./tiles.ts";

const FIXTURES = new URL("../../../tile-decoder/fixtures/", import.meta.url);
const DOMAINS = ["stars", "nearby_galaxies", "cosmic_web"];
const read = (file: string) => readFileSync(new URL(file, FIXTURES), "utf8");

const flip = (bytes: Uint8Array) => {
  bytes[0] = (bytes[0] ?? 0) ^ 0xff;
};

/** The bytes of the fixture tile at `address` of `domain`, if there is one. */
function tileBytes(domain: string, address: string): Uint8Array<ArrayBuffer> | undefined {
  const tiles = JSON.parse(read(`${domain}.tiles.json`)) as { tiles: { address: string; hex: string }[] };
  const found = tiles.tiles.find((t) => t.address === address);
  return found === undefined ? undefined : Uint8Array.from(found.hex.match(/../g) ?? [], (b) => Number.parseInt(b, 16));
}

/** Univ Server's answer to `path` on the fixture release, `corrupt` (domain/address) altered. */
function answer(path: string, corrupt?: string): Response {
  if (path === "/v1/scale-domains") return Response.json({ release: 2, domains: DOMAINS.map((name) => ({ name })) });
  const manifest = /^\/v1\/scale-domains\/(\w+)\/manifest$/.exec(path);
  if (manifest !== null) return new Response(read(`${manifest[1]}.manifest.json`));
  const [, domain = "", address = ""] = /^\/v1\/scale-domains\/(\w+)\/tiles\/(\w+)$/.exec(path) ?? [];
  const bytes = tileBytes(domain, address);
  if (bytes === undefined) return new Response("{}", { status: 404 });
  if (`${domain}/${address}` === corrupt) flip(bytes);
  return new Response(bytes);
}

/** Univ Server on the tile decoder's fixtures: the fixture release's three domains. */
function server(corrupt?: string): Http & { urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: (url) => {
      urls.push(url);
      return Promise.resolve(answer(url.split("?")[0] ?? "", corrupt));
    },
    sleep: () => Promise.resolve(),
    now: () => 0,
  };
}

async function settled(catalogue: Catalogue, changes: { count: number }, expected: number) {
  for (let i = 0; i < 200 && changes.count < expected; i += 1) await new Promise((r) => setTimeout(r, 1));
  return catalogue;
}

describe("the catalogue", () => {
  it("draws every root tile, counts each kind of value, and credits the drawn domains' sources", async () => {
    const http = server();
    const domains = await domainsOf(2, { http });
    const changes = { count: 0 };
    let batches = 0;
    const catalogue = new Catalogue(2, domains, { onBatch: () => (batches += 1), onChange: () => (changes.count += 1) }, { http });
    catalogue.update([0, 0, 0]);
    await settled(catalogue, changes, 3);
    expect(batches).toBe(3);
    expect(http.urls.filter((u) => u.includes("/tiles/")).every((u) => u.endsWith("release=2"))).toBe(true);
    expect(catalogue.points).toBeGreaterThan(0);
    expect([...catalogue.kinds.keys()].every((k) => ["measured", "estimated", "inferred", "reconstructed"].includes(k))).toBe(true);
    const credits = catalogue.credits();
    expect(credits.length).toBeGreaterThan(0);
    for (const credit of credits) {
      expect(credit.title).not.toBe("");
      expect(credit.citation).not.toBe("");
      expect(credit.credit).not.toBe("");
    }
  });

  it("leaves a tile that fails its SHA-256 undrawn and counts it", async () => {
    const http = server("stars/r");
    const domains = await domainsOf(2, { http });
    const changes = { count: 0 };
    const drawn: string[] = [];
    const catalogue = new Catalogue(2, domains, { onBatch: (b) => drawn.push(b.tile.id), onChange: () => (changes.count += 1) }, { http });
    catalogue.update([0, 0, 0]);
    await settled(catalogue, changes, 3);
    expect(catalogue.failed).toBe(1);
    expect(drawn).not.toContain("stars/r");
  });
});
