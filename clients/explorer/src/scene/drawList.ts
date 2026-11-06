// Which tiles' points a frame draws (draw-the-catalogues): those in sight and not turned off, each
// with the share of its points drawn and as opaque as its fade, none of which nothing would show.

import { drawnOf } from "../catalogue/lod.ts";
import { type Sight, type Sphere, inSight } from "./frustum.ts";
import type { Masks } from "./masks.ts";

/** A tile's points as a frame reads them. */
export interface Listed {
  id: string;
  domain: string;
  /** How many points it holds. */
  count: number;
  extent: Sphere;
}

/** A tile to draw: its first `count` points, `fade` as opaque as they would be. */
export interface ToDraw<B> {
  batch: B;
  count: number;
  fade: number;
}

/** What of each tile a frame draws: from what `share` and `fade` give of it by its id. */
export interface Drawing {
  sight: Sight;
  masks: Masks;
  share: (id: string) => number;
  fade: (id: string) => number;
}

/** The tiles of `batches` that `drawing` draws, with how many of each one's points, and how opaque. */
export function drawList<B extends Listed>(batches: Iterable<B>, { sight, masks, share, fade }: Drawing): ToDraw<B>[] {
  const list: ToDraw<B>[] = [];
  for (const batch of inSight(batches, sight, (b) => b.extent)) {
    if (masks.domains.has(batch.domain)) continue;
    const drawn = { batch, count: drawnOf(batch.count, share(batch.id)), fade: fade(batch.id) };
    if (drawn.count > 0 && drawn.fade > 0) list.push(drawn);
  }
  return list;
}
