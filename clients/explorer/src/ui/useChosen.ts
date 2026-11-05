// The chosen object (read-an-object): read from Univ Server, marked in the view, kept in the
// page's address, and flown to on arrival where asked.

import { useCallback, useEffect, useState } from "react";

import { ApiError } from "../api/client.ts";
import { type Choices, type ObjectRecord, nameOf, objectOf, placeOf } from "../api/objects.ts";
import { setInAddress } from "./address.ts";
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
  /** Named by the page's address, whose name the record must bear to head the panel. */
  fromAddress?: boolean;
}

/** The object the page's address names, if any, and the name it was found by. */
export function objectInAddress(search: string): { id: number; as: string | undefined } | undefined {
  const params = new URLSearchParams(search);
  const value = params.get("object");
  if (value === null || !/^\d+$/.test(value)) return undefined;
  return { id: Number(value), as: params.get("as") || undefined };
}

/** The page's address naming `request`'s object and the name it was found by, or no object. */
const keepInAddress = (request: Request | undefined): void =>
  setInAddress({ object: request === undefined ? undefined : String(request.id), as: request?.as });

function failure(id: number, error: unknown): ChosenObject {
  if (error instanceof ApiError) return { kind: "failed", id, status: error.status, title: error.title };
  return { kind: "failed", id, status: undefined, title: error instanceof Error ? error.message : String(error) };
}

/** Marks `record` in `world`, and flies to it where asked. */
function arrive(world: World, record: ObjectRecord, fly: boolean): void {
  const place = placeOf(record);
  world.chosen = place;
  world.controller.invalidate();
  if (fly && place !== undefined) world.controller.approach(place);
}

/** The chosen object of `release` in `world`, and the way to choose another. */
export function useChosen(release: number, world: World | undefined) {
  const [request, setRequest] = useState<Request | undefined>(() => {
    const named = objectInAddress(location.search);
    return named === undefined ? undefined : { ...named, fly: true, fromAddress: true };
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
      // A name the address gives heads the panel only where the record bears it: a link cannot
      // give an object a name it does not have.
      const as = request.fromAddress === true && request.as !== undefined ? nameOf(record, request.as) : request.as;
      if (as !== request.as) keepInAddress({ ...request, as });
      setChosen({ kind: "ready", record, as });
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
      // The same object again keeps its request, unless the name it was found by is given anew.
      const same = current?.id === id && options.record === undefined && (!("as" in options) || options.as === current.as);
      return same ? current : { id, ...options };
    });
  }, []);
  return { chosen, choose };
}
