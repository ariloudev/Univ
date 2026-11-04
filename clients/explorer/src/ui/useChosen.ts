// The chosen object (read-an-object): read from Univ Server, marked in the view, kept in the
// page's address, and flown to on arrival where asked.

import { useCallback, useEffect, useState } from "react";

import { ApiError } from "../api/client.ts";
import { type Choices, type ObjectRecord, objectOf, placeOf } from "../api/objects.ts";
import { viewingDistance } from "../scene/controls.ts";
import { norm } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** What the panel knows of the chosen object. */
export type ChosenObject =
  | { kind: "none" }
  | { kind: "loading"; id: number }
  | { kind: "ready"; record: ObjectRecord }
  | { kind: "choices"; choices: Choices }
  | { kind: "failed"; id: number; status: number | undefined; title: string };

/** How an object is chosen: its record where it is already read, and whether to fly to it
 * once it is (a picked point is flown to at once, by the scene). */
export interface ChooseOptions {
  record?: ObjectRecord | undefined;
  fly?: boolean | undefined;
}

interface Request extends ChooseOptions {
  id: number;
}

/** The object id the page's address names, if any. */
export function objectInAddress(search: string): number | undefined {
  const value = new URLSearchParams(search).get("object");
  return value !== null && /^\d+$/.test(value) ? Number(value) : undefined;
}

/** The page's address naming `id`, or naming no object. */
function keepInAddress(id: number | undefined): void {
  const url = new URL(location.href);
  if (id === undefined) url.searchParams.delete("object");
  else url.searchParams.set("object", String(id));
  history.replaceState(history.state, "", url);
}

function failure(id: number, error: unknown): ChosenObject {
  if (error instanceof ApiError) return { kind: "failed", id, status: error.status, title: error.title };
  return { kind: "failed", id, status: undefined, title: error instanceof Error ? error.message : String(error) };
}

/** Marks `record` in `world`, and flies to it where asked. */
function arrive(world: World, record: ObjectRecord, fly: boolean): void {
  const place = placeOf(record);
  world.chosen = place;
  world.controller.invalidate();
  if (fly && place !== undefined) world.controller.flyTo(place, viewingDistance(norm(place)));
}

/** The chosen object of `release` in `world`, and the way to choose another. */
export function useChosen(release: number, world: World | undefined) {
  const [request, setRequest] = useState<Request | undefined>(() => {
    const id = objectInAddress(location.search);
    return id === undefined ? undefined : { id, fly: true };
  });
  const [chosen, setChosen] = useState<ChosenObject>({ kind: "none" });
  useEffect(() => {
    keepInAddress(request?.id);
    if (world !== undefined) world.chosen = undefined;
    if (request === undefined || world === undefined) {
      setChosen({ kind: "none" });
      return;
    }
    const ready = (record: ObjectRecord) => {
      setChosen({ kind: "ready", record });
      arrive(world, record, request.fly === true);
    };
    if (request.record !== undefined) {
      ready(request.record);
      return;
    }
    const abort = new AbortController();
    setChosen({ kind: "loading", id: request.id });
    objectOf(request.id, { release, signal: abort.signal })
      .then((found) => (found.kind === "object" ? ready(found.record) : setChosen({ kind: "choices", choices: found })))
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setChosen(failure(request.id, error));
      });
    return () => abort.abort();
  }, [request, release, world]);
  /** Chooses object `id`, or none; choosing the object already chosen reads nothing again. */
  const choose = useCallback((id: number | undefined, options: ChooseOptions = {}) => {
    setRequest((current) => {
      if (id === undefined) return undefined;
      return current?.id === id && options.record === undefined ? current : { id, ...options };
    });
  }, []);
  return { chosen, choose };
}
