import { describe, expect, it } from "vitest";

import { answer, fakeHttp } from "../test/http.ts";
import { ApiError, UNREACHABLE, Unreachable, read, readJson, retryAfter, urlOf } from "./client.ts";
import { ReleaseList } from "./schemas.ts";

describe("read", () => {
  it("reads a refused read again after the seconds its Retry-After gives, saying so", async () => {
    const fake = fakeHttp(answer(429, {}, { "retry-after": "7" }), answer(200, { ok: true }));
    const waits: number[] = [];
    const response = await read("/v1/releases", { http: fake, onWait: (s) => waits.push(s) });
    expect(response.status).toBe(200);
    expect(fake.slept).toEqual([7000]);
    expect(waits).toEqual([7]);
    expect(fake.urls).toEqual(["/v1/releases", "/v1/releases"]);
  });

  it("makes every later read wait out the same pause", async () => {
    const fake = fakeHttp(answer(429, {}, { "retry-after": "5" }), answer(200, {}), answer(200, {}));
    await read("/v1/a", { http: fake });
    await read("/v1/b", { http: fake });
    expect(fake.slept).toEqual([5000]);
  });

  it("takes Retry-After as seconds, zero included, or as a date, and the window otherwise", () => {
    const now = Date.parse("2026-11-04T10:00:00Z");
    const with_ = (value: string) => answer(429, {}, { "retry-after": value });
    expect(retryAfter(with_("0"), now)).toBe(0);
    expect(retryAfter(with_("12"), now)).toBe(12);
    expect(retryAfter(with_("Wed, 04 Nov 2026 10:00:05 GMT"), now)).toBe(5);
    expect(retryAfter(with_("soon"), now)).toBe(60);
    expect(retryAfter(answer(429, {}), now)).toBe(60);
  });

  it("names the release on every read pinned to one", () => {
    expect(urlOf("/v1/scale-domains", 6)).toBe("/v1/scale-domains?release=6");
    expect(urlOf("/v1/objects?name=M31", 6)).toBe("/v1/objects?name=M31&release=6");
    expect(urlOf("/v1/releases")).toBe("/v1/releases");
  });

  it("surfaces a problem's status and title", async () => {
    const problem = answer(404, { status: 404, title: "No such object" });
    await expect(read("/v1/objects/9", { http: fakeHttp(problem) })).rejects.toEqual(
      new ApiError(404, "No such object"),
    );
  });

  it("calls Univ Server unreachable where the explorer's server says so, with its address", async () => {
    const proxied = answer(502, { type: UNREACHABLE, address: "http://127.0.0.1:8790" });
    await expect(read("/v1/releases", { http: fakeHttp(proxied) })).rejects.toEqual(
      new Unreachable("http://127.0.0.1:8790"),
    );
  });

  it("calls the explorer's own server unreachable when the read fails to connect", async () => {
    await expect(read("/v1/releases", { http: fakeHttp(new TypeError("refused")) })).rejects.toEqual(
      new Unreachable(location.origin),
    );
  });

  it("keeps a 5xx of a running Univ Server a problem, not unreachable", async () => {
    const crashed = new Response("<h1>Internal error</h1>", { status: 500, statusText: "Internal Server Error" });
    await expect(read("/v1/releases", { http: fakeHttp(crashed) })).rejects.toEqual(
      new ApiError(500, "Internal Server Error"),
    );
  });

  it("checks a JSON answer against its schema", async () => {
    const releases = {
      latest: 7,
      releases: [
        {
          number: 7,
          published_at: "2026-11-04T02:15:00Z",
          pruned_at: null,
          cosmology: { name: "Planck 2018", hubble_constant: 67.66, omega_matter: 0.3111 },
        },
      ],
    };
    const list = await readJson("/v1/releases", ReleaseList, { http: fakeHttp(answer(200, releases)) });
    expect(list.latest).toBe(7);
    await expect(
      readJson("/v1/releases", ReleaseList, { http: fakeHttp(answer(200, { latest: "seven" })) }),
    ).rejects.toThrow();
  });

  it("lets an aborted read fail as aborted, not as unreachable", async () => {
    const controller = new AbortController();
    controller.abort();
    const aborted = Object.assign(new Error("aborted"), { name: "AbortError" });
    await expect(read("/v1/releases", { http: fakeHttp(aborted), signal: controller.signal })).rejects.toBe(
      aborted,
    );
  });
});
