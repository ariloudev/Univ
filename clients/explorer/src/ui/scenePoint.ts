// A drawn point as the hover label and the chosen object know it, read from its tile.

import type { Batch, Domain } from "../catalogue/tiles.ts";
import { KINDS } from "../scene/kinds.ts";
import { type Vec3, norm } from "../scene/vec.ts";

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
export function pointOf(domains: ReadonlyMap<string, Domain>, batch: Batch, index: number): ScenePoint {
  const { tile, offsets } = batch;
  const place: Vec3 = [
    tile.centre[0] + (offsets[index * 3] ?? 0),
    tile.centre[1] + (offsets[index * 3 + 1] ?? 0),
    tile.centre[2] + (offsets[index * 3 + 2] ?? 0),
  ];
  const sourceIndex = batch.sources[index];
  const source = domains.get(tile.domain)?.manifest.sources.find((s) => s.index === sourceIndex);
  return {
    id: batch.ids[index] ?? 0,
    domain: tile.domain,
    place,
    distanceMpc: norm(place),
    source: source?.title ?? "an unlisted source",
    kind: KINDS[batch.kinds[index] ?? 0] ?? "unknown",
  };
}
