import { describe, expect, it } from "vitest";

import type { Http } from "./client.ts";
import { ANDROMEDA } from "../test/objects.ts";
import { type ObjectRecord, distancesOf, lookupPath, placeOf, search } from "./objects.ts";

function http(status: number, body: unknown): Http & { urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: (url) => {
      urls.push(url);
      return Promise.resolve(Response.json(body, { status }));
    },
    sleep: () => Promise.resolve(),
    now: () => 0,
  };
}

describe("finding an object", () => {
  it("looks a number up as an id, source:identifier as a designation, and anything else as a name", () => {
    expect(lookupPath(" 856254 ")).toBe("/v1/objects/856254");
    expect(lookupPath("hipparcos-2007:32349")).toBe("/v1/objects?source=hipparcos-2007&designation=32349");
    expect(lookupPath("Andromeda")).toBe("/v1/objects?name=Andromeda");
    expect(lookupPath("NGC 224")).toBe("/v1/objects?name=NGC%20224");
  });

  it("reads the object found, in the release chosen", async () => {
    const fake = http(200, ANDROMEDA);
    const found = await search("Andromeda", { http: fake, release: 7 });
    expect(found).toEqual({ kind: "object", record: ANDROMEDA });
    expect(fake.urls).toEqual(["/v1/objects?name=Andromeda&release=7"]);
  });

  it("lists the objects a name belongs to, to choose from", async () => {
    const choices = { objects: [{ id: 1, href: "/v1/objects/1", names: ["Twin"] }, { id: 2, href: "/v1/objects/2" }] };
    expect(await search("Twin", { http: http(300, choices) })).toEqual({
      kind: "choices",
      query: "Twin",
      objects: [
        { id: 1, names: ["Twin"] },
        { id: 2, names: [] },
      ],
    });
  });

  it("says nothing was found, keeping what was searched for", async () => {
    const problem = { status: 404, title: "No such object" };
    expect(await search("Nowhere", { http: http(404, problem) })).toEqual({ kind: "none", query: "Nowhere" });
  });
});

describe("an object's record", () => {
  it("picks out the preferred distance's measurement and every other distance", () => {
    const { preferred, others } = distancesOf(ANDROMEDA);
    expect(preferred?.source).toBe("cosmicflows-4-galaxies");
    expect(others.map((m) => m.source)).toEqual(["mcconnachie-2012"]);
  });

  it("places the object in megaparsecs, whether given in parsecs or megaparsecs", () => {
    expect(placeOf(ANDROMEDA)).toEqual([0.55, 0.1, 0.49]);
    const star = { ...ANDROMEDA, position: { ...ANDROMEDA.position, unit: "parsec", x: 2, y: 0, z: 0 } } as ObjectRecord;
    expect(placeOf(star)?.[0]).toBeCloseTo(2e-6);
    expect(placeOf({ ...ANDROMEDA, position: null })).toBeUndefined();
  });
});
