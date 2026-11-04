// @vitest-environment node
import { describe, expect, it } from "vitest";

import type { Http } from "../api/client.ts";
import { DOMAINS, bytesOf, fixtureText, fixtureTiles } from "../test/fixtures.ts";
import { Catalogue } from "./catalogue.ts";
import { domainsOf } from "./tiles.ts";

/** Univ Server's answer to `path` on the fixture release; `corrupt` (domain/address) altered,
 * `missing` (domain/address) answered 503. */
function answer(path: string, corrupt?: string, missing?: string): Response {
  if (path === "/v1/scale-domains") return Response.json({ release: 2, domains: DOMAINS.map((name) => ({ name })) });
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
function server(corrupt?: string, missing?: string): Http & { urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: (url) => {
      urls.push(url);
      return Promise.resolve(answer(url.split("?")[0] ?? "", corrupt, missing));
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

async function loaded(http: Http, drawn: string[]) {
  const domains = await domainsOf(2, { http });
  const catalogue = new Catalogue(2, domains, { onBatch: (b) => drawn.push(b.tile.id), onChange: () => undefined }, { http });
  catalogue.update([0, 0, 0]);
  return catalogue;
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

  it("does not count a tile Univ Server could not serve as failing its check", async () => {
    const drawn: string[] = [];
    const catalogue = await loaded(server(undefined, "stars/r"), drawn);
    await until(() => drawn.length >= 2);
    expect(catalogue.failed).toBe(0);
    expect(drawn).not.toContain("stars/r");
  });
});
