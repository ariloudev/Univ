// Reads Univ Server's public API (open-the-explorer). Every read goes to `/v1/...` on the
// explorer's own origin, which the dev server passes on to Univ Server.

import type { z } from "zod";

/** Univ Server answered with a problem (RFC 9457): its status and title. */
export class ApiError extends Error {
  readonly status: number;
  readonly title: string;

  constructor(status: number, title: string) {
    super(`${status} ${title}`);
    this.status = status;
    this.title = title;
  }
}

/** Univ Server could not be reached at `address`. */
export class Unreachable extends Error {
  readonly address: string;

  constructor(address: string) {
    super(`Univ Server is not answering at ${address}`);
    this.address = address;
  }
}

/** What a read needs from its surroundings, given in tests. */
export interface Http {
  fetch: (url: string, init?: RequestInit) => Promise<Response>;
  sleep: (ms: number) => Promise<void>;
}

const browser: Http = {
  fetch: (url, init) => fetch(url, init),
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};

/** The most times one read is refused for the rate limit before it gives up. */
const MAX_WAITS = 10;
/** The wait when a 429 gives no `Retry-After`: the limit's window. */
const DEFAULT_WAIT_SECONDS = 60;

export interface ReadOptions {
  /** The release read; the latest when absent. */
  release?: number;
  http?: Http;
  signal?: AbortSignal;
}

/** The URL of `path` pinned to `release`, if one is given. */
export function urlOf(path: string, release?: number): string {
  if (release === undefined) return path;
  return `${path}${path.includes("?") ? "&" : "?"}release=${release}`;
}

/** The seconds a 429's `Retry-After` gives, or the window when it gives none. */
export function retryAfter(response: Response): number {
  const seconds = Number(response.headers.get("retry-after"));
  return Number.isFinite(seconds) && seconds > 0 ? seconds : DEFAULT_WAIT_SECONDS;
}

/** A 5xx that is not one of Univ Server's own problems: the proxy found nothing to talk to. */
function proxyFailure(response: Response): boolean {
  const type = response.headers.get("content-type") ?? "";
  return response.status >= 500 && !type.includes("problem+json");
}

async function problemOf(response: Response): Promise<ApiError> {
  const problem: unknown = await response.json().catch(() => null);
  const title =
    typeof problem === "object" && problem !== null && "title" in problem
      ? String(problem.title)
      : response.statusText;
  return new ApiError(response.status, title);
}

/** The answer to a GET of `path`, after waiting out any rate limit. */
export async function read(path: string, options: ReadOptions = {}): Promise<Response> {
  const http = options.http ?? browser;
  const url = urlOf(path, options.release);
  const init: RequestInit = options.signal === undefined ? {} : { signal: options.signal };
  for (let waits = 0; ; waits += 1) {
    const response = await http.fetch(url, init).catch((error: unknown) => {
      if (options.signal?.aborted === true) throw error;
      throw new Unreachable(__UNIV_SERVER__);
    });
    if (response.status === 429 && waits < MAX_WAITS) {
      await http.sleep(retryAfter(response) * 1000);
      continue;
    }
    if (proxyFailure(response)) throw new Unreachable(__UNIV_SERVER__);
    if (!response.ok) throw await problemOf(response);
    return response;
  }
}

/** The JSON of a GET of `path`, checked against `schema`. */
export async function readJson<T>(
  path: string,
  schema: z.ZodType<T>,
  options: ReadOptions = {},
): Promise<T> {
  const response = await read(path, options);
  return schema.parse(await response.json());
}
