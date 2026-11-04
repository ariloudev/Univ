// A drawn point as the hover label and the chosen object know it, read from its tile.

import type { Batch, Domain } from "../catalogue/tiles.ts";
import { KINDS } from "../scene/kinds.ts";
import { type Pointer, pick } from "../scene/pick.ts";
import { type Vec3, norm } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** A drawn point: its object id, scale domain, place, and the source and kind of its distance. */
export interface ScenePoint {
  id: number;
  domain: string;
  place: Vec3;
  distanceMpc: number;
  source: string;
  kind: string;
}

/** The point `index` of `batch`. */
export function pointOf(domainOf: (name: string) => Domain | undefined, batch: Batch, index: number): ScenePoint {
  const { tile, offsets } = batch;
  const place: Vec3 = [
    tile.centre[0] + (offsets[index * 3] ?? 0),
    tile.centre[1] + (offsets[index * 3 + 1] ?? 0),
    tile.centre[2] + (offsets[index * 3 + 2] ?? 0),
  ];
  const sourceIndex = batch.sources[index];
  const source = domainOf(tile.domain)?.manifest.sources.find((s) => s.index === sourceIndex);
  return {
    id: batch.ids[index] ?? 0,
    domain: tile.domain,
    place,
    distanceMpc: norm(place),
    source: source?.title ?? "an unlisted source",
    kind: KINDS[batch.kinds[index] ?? 0] ?? "unknown",
  };
}

/** The point of `world` under `pointer`, if any. */
export function pointUnder(world: World, pointer: Pointer): ScenePoint | undefined {
  const { controller, catalogue } = world;
  const picked = pick(world.pickable, controller.view, controller.toFrame, pointer);
  const entry = picked === undefined ? undefined : world.pickable[picked.batch];
  if (picked === undefined || entry === undefined || catalogue === undefined) return undefined;
  return pointOf((name) => catalogue.domain(name), entry.batch, picked.index);
}
