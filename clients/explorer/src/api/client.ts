// Reads Univ Server's public API (open-the-explorer). Every read goes to `/v1/...` on the
// explorer's own origin, whose server passes it on to Univ Server and, where Univ Server does
// not answer, answers itself with the problem `UNREACHABLE`.

import type { z } from "zod";

/** The problem type the explorer's own server answers with when Univ Server does not answer. */
export const UNREACHABLE = "urn:univ-explorer:problem:server-unreachable";

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

/** Univ Server, or the explorer's own server, could not be reached at `address`. */
export class Unreachable extends Error {
  readonly address: string;

  constructor(address: string) {
    super(`No answer from ${address}`);
    this.address = address;
  }
}

/** What reads need from their surroundings, given in tests. */
export interface Http {
  fetch: (url: string, init?: RequestInit) => Promise<Response>;
  /** Waits `ms`, or rejects when `signal` is aborted first. */
  sleep: (ms: number, signal?: AbortSignal) => Promise<void>;
  now: () => number;
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener("abort", () => {
      clearTimeout(timer);
      reject(signal.reason as Error);
    });
  });
}

const browser: Http = { fetch: (url, init) => fetch(url, init), sleep, now: () => Date.now() };

/** When reads through each `Http` may resume after a 429: one pause shared by every read. */
const resumeAt = new WeakMap<Http, number>();

/** The most times one read is refused for the rate limit before it gives up. */
const MAX_WAITS = 10;
/** The wait when a 429 gives no `Retry-After` that reads: the limit's window. */
const DEFAULT_WAIT_SECONDS = 60;

export interface ReadOptions {
  /** The release read; the latest when absent. */
  release?: number;
  http?: Http;
  signal?: AbortSignal;
  /** Told the seconds each wait for the rate limit lasts, as it begins. */
  onWait?: (seconds: number) => void;
  /** Statuses besides 2xx answered with the response rather than as a problem. */
  accept?: readonly number[];
}

/** The URL of `path` pinned to `release`, if one is given. */
export function urlOf(path: string, release?: number): string {
  if (release === undefined) return path;
  return `${path}${path.includes("?") ? "&" : "?"}release=${release}`;
}

/** The seconds a 429's `Retry-After` gives, as seconds or as a date; the window otherwise. */
export function retryAfter(response: Response, now: number): number {
  const given = response.headers.get("retry-after");
  if (given === null) return DEFAULT_WAIT_SECONDS;
  const seconds = Number(given);
  if (given.trim() !== "" && Number.isFinite(seconds) && seconds >= 0) return seconds;
  const date = Date.parse(given);
  return Number.isNaN(date) ? DEFAULT_WAIT_SECONDS : Math.max(0, (date - now) / 1000);
}

async function problemOf(response: Response): Promise<Error> {
  const problem: unknown = await response.json().catch(() => null);
  const member = (name: string): unknown =>
    typeof problem === "object" && problem !== null && name in problem
      ? (problem as Record<string, unknown>)[name]
      : undefined;
  if (member("type") === UNREACHABLE) return new Unreachable(String(member("address")));
  const title = member("title");
  return new ApiError(response.status, typeof title === "string" ? title : response.statusText);
}

/** Waits until reads through `http` may resume, telling `onWait` of a wait. */
async function paused(http: Http, options: ReadOptions): Promise<void> {
  const ms = (resumeAt.get(http) ?? 0) - http.now();
  if (ms <= 0) return;
  options.onWait?.(Math.ceil(ms / 1000));
  await http.sleep(ms, options.signal);
}

/** The answer to a GET of `path`, after waiting out any rate limit. */
export async function read(path: string, options: ReadOptions = {}): Promise<Response> {
  const http = options.http ?? browser;
  const url = urlOf(path, options.release);
  const init: RequestInit = options.signal === undefined ? {} : { signal: options.signal };
  for (let waits = 0; ; waits += 1) {
    await paused(http, options);
    const response = await http.fetch(url, init).catch((error: unknown) => {
      if (options.signal?.aborted === true) throw error;
      throw new Unreachable(location.origin);
    });
    if (response.status === 429 && waits < MAX_WAITS) {
      const until = http.now() + retryAfter(response, http.now()) * 1000;
      resumeAt.set(http, Math.max(resumeAt.get(http) ?? 0, until));
      continue;
    }
    if (!response.ok && options.accept?.includes(response.status) !== true) throw await problemOf(response);
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
