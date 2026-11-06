// A drawn point as the hover label and the chosen object know it, read from its tile.

import { drawnOf } from "../catalogue/lod.ts";
import type { Batch, Domain } from "../catalogue/tiles.ts";
import type { StarColourIndex } from "../scene/starColour.ts";
import { KINDS } from "../scene/kinds.ts";
import type { Shown } from "../scene/masks.ts";
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
  /** Its colour index and the bands it lies between, where its tile gives one. */
  colour?: StarColourIndex | undefined;
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
  const manifest = domainOf(tile.domain)?.manifest;
  const source = manifest?.sources.find((s) => s.index === sourceIndex);
  return {
    id: batch.ids[index] ?? 0,
    domain: tile.domain,
    place,
    distanceMpc: norm(place),
    source: source?.title ?? "an unlisted source",
    kind: KINDS[batch.kinds[index] ?? 0] ?? "unknown",
    colour: colourOf(batch, index, manifest?.encoding.colour_bands ?? []),
  };
}

/** The colour index of the point `index` of `batch`, if its tile gives one, its bands named by
 * the manifest's `names`. */
function colourOf(batch: Batch, index: number, names: readonly StarColourIndex["bands"][]): ScenePoint["colour"] {
  const bands = names[(batch.stars?.bands[index] ?? 0) - 1];
  const hundredths = batch.stars?.hundredths[index];
  return bands === undefined || hundredths === undefined ? undefined : { index: hundredths / 100, bands };
}

/** The points that both `shown` and `drawn` leave drawn. */
function both(shown: Shown | undefined, drawn: ((index: number) => boolean) | undefined): Shown | undefined {
  if (drawn === undefined || shown === "none") return shown;
  if (shown === undefined || shown === "all") return drawn;
  return (i) => shown(i) && drawn(i);
}

/** The point of `world` under `pointer`, if any, of the points drawn. */
export function pointUnder(world: World, pointer: Pointer): ScenePoint | undefined {
  const { controller, catalogue } = world;
  const drawn = world.pickable.map((p) => ({
    ...p,
    count: drawnOf(p.offsets.length / 3, catalogue?.share(p.batch.tile.id) ?? 1),
    shown: both(p.shown, catalogue?.drawnPoints(p.batch)),
  }));
  const picked = pick(drawn, controller.view, controller.toFrame, pointer);
  const entry = picked === undefined ? undefined : world.pickable[picked.batch];
  if (picked === undefined || entry === undefined || catalogue === undefined) return undefined;
  return pointOf((name) => catalogue.domain(name), entry.batch, picked.index);
}
