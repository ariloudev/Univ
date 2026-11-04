import { describe, expect, it, vi } from "vitest";

import { ApiError, type Http, Unreachable, read, readJson, retryAfter, urlOf } from "./client.ts";
import { ReleaseList } from "./schemas.ts";

function answer(status: number, body: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers });
}

function http(...responses: (Response | Error)[]): Http & { urls: string[]; slept: number[] } {
  const urls: string[] = [];
  const slept: number[] = [];
  return {
    urls,
    slept,
    fetch: (url) => {
      urls.push(url);
      const next = responses.shift();
      if (next === undefined) throw new Error("no more answers");
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(next);
    },
    sleep: (ms) => {
      slept.push(ms);
      return Promise.resolve();
    },
  };
}

describe("read", () => {
  it("reads a refused read again after the seconds its Retry-After gives", async () => {
    const fake = http(answer(429, {}, { "retry-after": "7" }), answer(200, { ok: true }));
    const response = await read("/v1/releases", { http: fake });
    expect(response.status).toBe(200);
    expect(fake.slept).toEqual([7000]);
    expect(fake.urls).toEqual(["/v1/releases", "/v1/releases"]);
  });

  it("waits the limit's window when a 429 gives no Retry-After", () => {
    expect(retryAfter(answer(429, {}))).toBe(60);
  });

  it("names the release on every read pinned to one", () => {
    expect(urlOf("/v1/scale-domains", 6)).toBe("/v1/scale-domains?release=6");
    expect(urlOf("/v1/objects?name=M31", 6)).toBe("/v1/objects?name=M31&release=6");
    expect(urlOf("/v1/releases")).toBe("/v1/releases");
  });

  it("surfaces a problem's status and title", async () => {
    const problem = answer(404, { status: 404, title: "No such object" }, {
      "content-type": "application/problem+json",
    });
    await expect(read("/v1/objects/9", { http: http(problem) })).rejects.toEqual(
      new ApiError(404, "No such object"),
    );
  });

  it("calls a failed connection, or a proxy's own 5xx, unreachable", async () => {
    await expect(read("/v1/releases", { http: http(new TypeError("refused")) })).rejects.toBeInstanceOf(
      Unreachable,
    );
    const proxy = new Response("Bad gateway", { status: 502 });
    await expect(read("/v1/releases", { http: http(proxy) })).rejects.toBeInstanceOf(Unreachable);
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
    const list = await readJson("/v1/releases", ReleaseList, { http: http(answer(200, releases)) });
    expect(list.latest).toBe(7);
    await expect(
      readJson("/v1/releases", ReleaseList, { http: http(answer(200, { latest: "seven" })) }),
    ).rejects.toThrow();
  });

  it("lets an aborted read fail as aborted, not as unreachable", async () => {
    const controller = new AbortController();
    controller.abort();
    const aborted = new DOMException("aborted", "AbortError");
    const fetch = vi.fn<Http["fetch"]>().mockRejectedValue(aborted);
    await expect(
      read("/v1/releases", { http: { fetch, sleep: () => Promise.resolve() }, signal: controller.signal }),
    ).rejects.toBe(aborted);
  });
});
