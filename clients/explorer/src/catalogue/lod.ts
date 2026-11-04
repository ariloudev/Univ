// Which tiles the view needs next (draw-the-catalogues). Every point of a domain is stored in
// exactly one node, and an inner node keeps a fixed fraction of the points under it, so a view
// gains detail by adding the children of the nodes it holds: nothing is drawn twice.

import { type Vec3, norm, sub } from "../scene/vec.ts";

/** One node of a domain's octree, in megaparsecs. */
export interface Tile {
  /** `domain/address`, unique across domains. */
  id: string;
  domain: string;
  address: string;
  /** The node it refines; none for a root. */
  parent: string | undefined;
  centre: Vec3;
  /** The length of its cube's edge. */
  edge: number;
  points: number;
}

/** The most points the explorer holds at once. */
export const POINT_BUDGET = 3_000_000;

/** How large `tile` looks from `eye`: its edge over its distance, its cube's nearest reach. */
export function apparentSize(tile: Tile, eye: Vec3): number {
  const reach = norm(sub(tile.centre, eye)) - tile.edge * 0.87;
  return tile.edge / Math.max(reach, tile.edge * 0.05);
}

/** The tiles to fetch next, at most `count`: every root first, then the children of the tiles
 * held, largest on screen first, while they fit in the budget. A release within the budget is
 * fetched whole, so that no region stays sparser than its neighbours once loading ends. */
export function nextTiles<T extends Tile>(
  tiles: readonly T[],
  held: ReadonlySet<string>,
  pending: ReadonlySet<string>,
  heldPoints: number,
  eye: Vec3,
  count: number,
): T[] {
  const open = tiles.filter((t) => !held.has(t.id) && !pending.has(t.id));
  const roots = open.filter((t) => t.parent === undefined);
  if (roots.length > 0) return roots.slice(0, count);
  const ranked = open
    .filter((t) => t.parent !== undefined && held.has(t.parent))
    .map((t) => ({ tile: t, size: apparentSize(t, eye) }))
    .sort((a, b) => b.size - a.size);
  const chosen: T[] = [];
  let points = heldPoints;
  for (const { tile } of ranked) {
    if (chosen.length >= count) break;
    if (points + tile.points > POINT_BUDGET) continue;
    points += tile.points;
    chosen.push(tile);
  }
  return chosen;
}
