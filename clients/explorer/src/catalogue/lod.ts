// Which tiles the view needs (draw-the-catalogues). Every point of a domain is stored in exactly
// one node, and an inner node keeps a fixed fraction of the points under it, so a view gains
// detail by adding the children of the nodes it holds: nothing is drawn twice. A node is
// refined while it is in view and its points lie farther apart on screen than the screen can
// show; when the budget needs room, the held nodes the view needs least are dropped, never a
// root nor a node whose children are held.

import type { Sight } from "../scene/frustum.ts";
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

/** A node is refined while its points lie farther apart on screen than this, pixels. */
export const REFINE_SPACING = 3;

/** How large `tile` looks from `eye`: its edge over its distance, its cube's nearest reach. */
export function apparentSize(tile: Tile, eye: Vec3): number {
  const reach = norm(sub(tile.centre, eye)) - tile.edge * 0.87;
  return tile.edge / Math.max(reach, tile.edge * 0.05);
}

/** The radius of the sphere about a tile's cube: half its diagonal. */
export const radiusOf = (tile: Tile): number => tile.edge * (Math.sqrt(3) / 2);

/** How far apart a tile's points lie on screen, pixels: its cube's size on screen over the
 * points along an edge. */
export const spacing = (tile: Tile, sight: Sight): number =>
  (apparentSize(tile, sight.eye) * sight.focal) / Math.cbrt(Math.max(1, tile.points));

/** Whether the view needs the children of `tile`. */
const refines = (tile: Tile, sight: Sight): boolean =>
  sight.inView(tile.centre, radiusOf(tile)) && spacing(tile, sight) > REFINE_SPACING;

/** What a catalogue holds, as the plan reads it. */
export interface Holding {
  /** Tiles settled: drawn, or failed and given up. */
  settled: ReadonlySet<string>;
  /** Tiles drawn, which may be dropped. */
  drawn: ReadonlySet<string>;
  pending: ReadonlySet<string>;
  /** The points of the tiles drawn and on their way. */
  points: number;
}

/** The tiles to fetch next, at most `count`, and the drawn tiles to drop to make room for them. */
export interface Plan<T> {
  fetch: T[];
  drop: T[];
}

/** How much the view needs a tile: in view before out of it, then the larger on screen. */
function need(tile: Tile, sight: Sight): [number, number] {
  return [sight.inView(tile.centre, radiusOf(tile)) ? 1 : 0, apparentSize(tile, sight.eye)];
}

const less = (a: [number, number], b: [number, number]) => a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]);

/** The drawn tiles that may be dropped, the least needed first: no root, and none with a child
 * drawn or on its way. */
function droppable<T extends Tile>(tiles: readonly T[], holding: Holding, sight: Sight) {
  const parents = new Set<string>();
  for (const t of tiles) {
    if (t.parent !== undefined && (holding.settled.has(t.id) || holding.pending.has(t.id))) parents.add(t.parent);
  }
  return tiles
    .filter((t) => t.parent !== undefined && holding.drawn.has(t.id) && !parents.has(t.id))
    .map((tile) => ({ tile, need: need(tile, sight) }))
    .sort((a, b) => (less(a.need, b.need) ? -1 : less(b.need, a.need) ? 1 : 0));
}

/** What to fetch and drop for `sight`: every root first; then the children of the tiles the view
 * refines, in view, the largest on screen first, dropping for each the tiles needed less than it
 * while the budget has no room. */
export function plan<T extends Tile>(
  tiles: readonly T[],
  holding: Holding,
  sight: Sight,
  count: number,
  budget = POINT_BUDGET,
): Plan<T> {
  const open = tiles.filter((t) => !holding.settled.has(t.id) && !holding.pending.has(t.id));
  const roots = open.filter((t) => t.parent === undefined);
  if (roots.length > 0) return { fetch: roots.slice(0, count), drop: [] };
  const byId = new Map(tiles.map((t) => [t.id, t]));
  const ranked = open
    .filter((t) => {
      const parent = t.parent === undefined ? undefined : byId.get(t.parent);
      return parent !== undefined && holding.settled.has(parent.id) && refines(parent, sight) && sight.inView(t.centre, radiusOf(t));
    })
    .map((tile) => ({ tile, need: need(tile, sight) }))
    .sort((a, b) => b.need[1] - a.need[1]);
  const room = new Room(droppable(tiles, holding, sight), holding.points, budget);
  const fetch: T[] = [];
  for (const { tile, need: wanted } of ranked) {
    if (fetch.length >= count) break;
    if (room.take(tile, wanted)) fetch.push(tile);
  }
  return { fetch, drop: room.drop };
}

/** The budget's room as tiles are taken, dropping the spare tiles needed less than each. */
class Room<T extends Tile> {
  readonly drop: T[] = [];
  private readonly dropped = new Set<string>();
  /** The parents of the tiles taken, which stay. */
  private readonly keep = new Set<string>();
  private next = 0;
  private points: number;

  constructor(
    private readonly spare: readonly { tile: T; need: [number, number] }[],
    points: number,
    private readonly budget: number,
  ) {
    this.points = points;
  }

  /** Whether `tile`, needed as `wanted`, fits once what may make room for it is dropped. */
  take(tile: T, wanted: [number, number]): boolean {
    if (tile.parent !== undefined && this.dropped.has(tile.parent)) return false;
    if (tile.parent !== undefined) this.keep.add(tile.parent);
    while (this.points + tile.points > this.budget && this.dropOne(wanted)) {
      // dropOne made some room, or skipped a tile that stays.
    }
    if (this.points + tile.points > this.budget) return false;
    this.points += tile.points;
    return true;
  }

  /** Drops the next spare tile needed less than `wanted`; false when there is none. */
  private dropOne(wanted: [number, number]): boolean {
    const candidate = this.spare[this.next];
    if (candidate === undefined || !less(candidate.need, wanted)) return false;
    this.next += 1;
    if (this.keep.has(candidate.tile.id)) return true;
    this.dropped.add(candidate.tile.id);
    this.drop.push(candidate.tile);
    this.points -= candidate.tile.points;
    return true;
  }
}
