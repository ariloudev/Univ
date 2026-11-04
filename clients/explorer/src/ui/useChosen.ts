// The chosen object (read-an-object): read from Univ Server, marked in the view, kept in the
// page's address, and flown to when asked.

import { useCallback, useEffect, useState } from "react";

import { ApiError } from "../api/client.ts";
import { type ObjectRecord, objectOf, placeOf } from "../api/objects.ts";
import { viewingDistance } from "../scene/controls.ts";
import { type Vec3, norm } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** What the panel knows of the chosen object. */
export type ChosenObject =
  | { kind: "none" }
  | { kind: "loading"; id: number }
  | { kind: "ready"; record: ObjectRecord }
  | { kind: "failed"; id: number; status: number | undefined; title: string };

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

/** Marks `place` in `world` as the chosen object's, or marks none. */
function mark(world: World, place: Vec3 | undefined): void {
  world.chosen = place;
  world.renderer.setMarkers("chosen", place === undefined ? undefined : { centre: place, offsets: new Float32Array(3) });
  world.controller.invalidate();
}

/** Flies `world` to `record`'s place, if it has one. */
function flyTo(world: World, record: ObjectRecord): void {
  const place = placeOf(record);
  if (place !== undefined) world.controller.flyTo(place, viewingDistance(norm(place)));
}

/** The chosen object of `release` in `world`, and the way to choose another. */
export function useChosen(release: number, world: World | undefined) {
  const [request, setRequest] = useState(() => {
    const id = objectInAddress(location.search);
    return id === undefined ? undefined : { id, fly: true };
  });
  const [chosen, setChosen] = useState<ChosenObject>({ kind: "none" });
  useEffect(() => {
    keepInAddress(request?.id);
    if (world !== undefined) mark(world, undefined);
    if (request === undefined || world === undefined) {
      setChosen({ kind: "none" });
      return;
    }
    const abort = new AbortController();
    setChosen({ kind: "loading", id: request.id });
    objectOf(request.id, { release, signal: abort.signal })
      .then((record) => {
        setChosen({ kind: "ready", record });
        mark(world, placeOf(record));
        if (request.fly) flyTo(world, record);
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setChosen(failure(request.id, error));
      });
    return () => abort.abort();
  }, [request, release, world]);
  const choose = useCallback((id: number | undefined, fly: boolean) => {
    setRequest(id === undefined ? undefined : { id, fly });
  }, []);
  return { chosen, choose };
}
