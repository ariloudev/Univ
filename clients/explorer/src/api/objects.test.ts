import { describe, expect, it } from "vitest";

import { answer, fakeHttp } from "../test/http.ts";
import { ANDROMEDA } from "../test/objects.ts";
import { type ObjectRecord, distancesOf, lookupPath, objectOf, placeOf, search } from "./objects.ts";

const http = (status: number, body: unknown) => fakeHttp(answer(status, body));

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
    const problem = { status: 404, title: "No such object", type: "urn:univ:problem:object-not-found" };
    expect(await search("Nowhere", { http: http(404, problem) })).toEqual({ kind: "none", query: "Nowhere" });
  });

  it("keeps any other refusal a problem, such as a source that does not exist", async () => {
    const problem = { status: 404, title: "No such source", type: "urn:univ:problem:source-not-found" };
    await expect(search("nosuch:1", { http: http(404, problem) })).rejects.toThrow("404 No such source");
  });

  it("lists the objects that replaced an id that was split", async () => {
    const choices = { objects: [{ id: 3, href: "/v1/objects/3" }, { id: 4, href: "/v1/objects/4" }] };
    expect(await objectOf(12, { http: http(300, choices) })).toEqual({
      kind: "choices",
      query: "12",
      objects: [
        { id: 3, names: [] },
        { id: 4, names: [] },
      ],
    });
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
