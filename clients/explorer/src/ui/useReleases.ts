import { useCallback, useEffect, useState } from "react";

import { ApiError, Unreachable, readJson } from "../api/client.ts";
import { ReleaseList, type ReleaseSummary } from "../api/schemas.ts";

/** What the explorer knows of Univ Server's releases. */
export type Releases =
  | { kind: "loading" }
  | { kind: "unreachable"; address: string }
  | { kind: "failed"; message: string }
  | { kind: "ready"; releases: ReleaseSummary[]; latest: number };

function failure(error: unknown): Releases {
  if (error instanceof Unreachable) return { kind: "unreachable", address: error.address };
  if (error instanceof ApiError) return { kind: "failed", message: `${error.status} ${error.title}` };
  return { kind: "failed", message: String(error) };
}

/** The releases that can be drawn: published and not pruned. */
function drawable(releases: ReleaseSummary[]): ReleaseSummary[] {
  return releases.filter((release) => release.pruned_at === null || release.pruned_at === undefined);
}

/** Univ Server's releases, read when mounted and again on `retry`. */
export function useReleases(): { state: Releases; retry: () => void } {
  const [state, setState] = useState<Releases>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setState({ kind: "loading" });
    readJson("/v1/releases", ReleaseList, { signal: controller.signal })
      .then((list) => {
        setState({ kind: "ready", releases: drawable(list.releases), latest: list.latest });
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted) setState(failure(error));
      });
    return () => {
      controller.abort();
    };
  }, [attempt]);
  const retry = useCallback(() => {
    setAttempt((n) => n + 1);
  }, []);
  return { state, retry };
}
