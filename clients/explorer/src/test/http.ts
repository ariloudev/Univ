// An `Http` for tests, answering in turn and keeping the URLs it was asked for; its clock moves
// only by its sleeps.

import type { Http } from "../api/client.ts";

/** A JSON answer with `status`. */
export function answer(status: number, body: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers });
}

export function fakeHttp(...responses: (Response | Error)[]): Http & { urls: string[]; slept: number[] } {
  const urls: string[] = [];
  const slept: number[] = [];
  let clock = 1_000_000;
  return {
    urls,
    slept,
    fetch: (url) => {
      urls.push(url);
      const next = responses.shift();
      if (next === undefined) throw new Error("no more answers");
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(next);
    },
    sleep: (ms, signal) => {
      if (signal?.aborted === true) return Promise.reject(signal.reason as Error);
      slept.push(ms);
      clock += ms;
      return Promise.resolve();
    },
    now: () => clock,
  };
}
