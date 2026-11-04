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
  | { kind: "ready"; record: ObjectRecord; as?: string | undefined }
  | { kind: "choices"; choices: Choices }
  | { kind: "failed"; id: number; status: number | undefined; title: string };

/** How an object is chosen: its record where it is already read, the name it was found by,
 * and whether to fly to it once it is (a picked point is flown to at once, by the scene). */
export interface ChooseOptions {
  record?: ObjectRecord | undefined;
  as?: string | undefined;
  fly?: boolean | undefined;
}

interface Request extends ChooseOptions {
  id: number;
}

/** The object the page's address names, if any, and the name it was found by. */
export function objectInAddress(search: string): { id: number; as?: string } | undefined {
  const params = new URLSearchParams(search);
  const value = params.get("object");
  if (value === null || !/^\d+$/.test(value)) return undefined;
  const as = params.get("as");
  return as === null || as === "" ? { id: Number(value) } : { id: Number(value), as };
}

/** The page's address naming `request`'s object and the name it was found by, or no object. */
function keepInAddress(request: Request | undefined): void {
  const url = new URL(location.href);
  url.searchParams.delete("as");
  if (request === undefined) url.searchParams.delete("object");
  else url.searchParams.set("object", String(request.id));
  if (request?.as !== undefined) url.searchParams.set("as", request.as);
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
    const named = objectInAddress(location.search);
    return named === undefined ? undefined : { ...named, fly: true };
  });
  const [chosen, setChosen] = useState<ChosenObject>({ kind: "none" });
  useEffect(() => {
    keepInAddress(request);
    if (world !== undefined) world.chosen = undefined;
    if (request === undefined || world === undefined) {
      setChosen({ kind: "none" });
      return;
    }
    const ready = (record: ObjectRecord) => {
      setChosen({ kind: "ready", record, as: request.as });
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
      return current?.id === id && options.record === undefined && options.as === undefined ? current : { id, ...options };
    });
  }, []);
  return { chosen, choose };
}
