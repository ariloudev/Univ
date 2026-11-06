// Which tiles the view needs (draw-the-catalogues). Every point of a domain is stored in exactly
// one node, and an inner node keeps a fixed fraction of the points under it, so a view gains
// detail by adding the children of the nodes it holds: nothing is drawn twice. The view wants
// every root and, the largest on screen first, the children in view of each wanted node whose
// points lie farther apart on screen than the screen can show, all of a node's together, within
// the budget. What it wants is fetched; what it holds and does not want is dropped when the
// budget needs room, out of view first, then the smallest on screen, never a root nor a node
// whose children are held.

import type { Sight, Sphere } from "../scene/frustum.ts";
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
export const POINT_BUDGET = 6_000_000;

/** A node is refined while its points lie farther apart on screen than this, pixels. */
export const REFINE_SPACING = 3;

/** A node whose children are held keeps them while its points lie farther apart on screen than
 * this, pixels: below the spacing that refines it, so that a view moving a little about the
 * spacing does not drop the children it has just fetched. */
export const KEEP_SPACING = 2;

/** How much larger than it is a family that holds a tile counts when the budget has room for
 * only some families: so that a view moving a little, which changes the families' sizes on
 * screen a little, does not trade a held family for one of nearly its size. */
export const HELD_PREFERENCE = 1.5;

/** How far outside the view a held tile still counts as in it, radians (1 degree): so that a
 * view turning a little does not drop a tile at its edge and fetch it again as it turns back. */
export const HELD_VIEW_MARGIN = Math.PI / 180;

/** How large `tile` looks from `eye`: its edge over its distance, its cube's nearest reach. */
export function apparentSize(tile: Tile, eye: Vec3): number {
  const reach = norm(sub(tile.centre, eye)) - radiusOf(tile);
  return tile.edge / Math.max(reach, tile.edge * 0.05);
}

/** The radius of the sphere about a tile's cube: half its diagonal. */
const radiusOf = (tile: Tile): number => tile.edge * (Math.sqrt(3) / 2);

/** Where a tile's points lie: the sphere about their box, which holds every one, and the sphere
 * about their mean that holds most of them. A domain's cubes are far larger than its data (the
 * cosmic web's root is 16 Gpc across, its surveys a few), and a few far objects stretch a box (the
 * root's reach 6 Gpc while 90 % lie within 1.3 Gpc), so once a tile's points are read their
 * sphere decides whether it is in view and their core how far apart they lie on screen; before,
 * the tile's cube decides both. */
export type Extent = Sphere & { core: Sphere };

/** The share of a tile's points its core holds. */
const CORE = 0.9;

/** The sphere about a tile's cube, as its extent before its points are read. */
function cubeExtent(tile: Tile): Extent {
  const sphere = { centre: tile.centre, radius: radiusOf(tile) };
  return { ...sphere, core: sphere };
}

/** The extent of `offsets` (x, y, z triples) from `centre`. */
export function extentOf(offsets: Float32Array, centre: Vec3): Extent {
  const n = Math.floor(offsets.length / 3);
  if (n === 0) return { centre, radius: 0, core: { centre, radius: 0 } };
  let [minX, minY, minZ] = [Infinity, Infinity, Infinity];
  let [maxX, maxY, maxZ] = [-Infinity, -Infinity, -Infinity];
  let [sumX, sumY, sumZ] = [0, 0, 0];
  for (let i = 0; i < n * 3; i += 3) {
    const x = offsets[i] ?? 0;
    const y = offsets[i + 1] ?? 0;
    const z = offsets[i + 2] ?? 0;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
    minY = Math.min(minY, y);
    maxY = Math.max(maxY, y);
    minZ = Math.min(minZ, z);
    maxZ = Math.max(maxZ, z);
    sumX += x;
    sumY += y;
    sumZ += z;
  }
  const [mx, my, mz] = [sumX / n, sumY / n, sumZ / n];
  // Squared distances from the mean sort as the distances do.
  const reach = new Float64Array(n);
  for (let i = 0; i < n; i += 1) {
    const dx = (offsets[i * 3] ?? 0) - mx;
    const dy = (offsets[i * 3 + 1] ?? 0) - my;
    const dz = (offsets[i * 3 + 2] ?? 0) - mz;
    reach[i] = dx * dx + dy * dy + dz * dz;
  }
  reach.sort();
  return {
    centre: [centre[0] + (minX + maxX) / 2, centre[1] + (minY + maxY) / 2, centre[2] + (minZ + maxZ) / 2],
    radius: Math.hypot(maxX - minX, maxY - minY, maxZ - minZ) / 2,
    core: { centre: [centre[0] + mx, centre[1] + my, centre[2] + mz], radius: Math.sqrt(reach[Math.ceil(CORE * n) - 1] ?? 0) },
  };
}

/** How far apart a tile's points lie on screen, pixels: its points spread over the square their
 * core covers on screen, every point of its depth landing on that square. */
export function spacing(tile: Tile, sight: Sight, extent = cubeExtent(tile)): number {
  const { centre, radius } = extent.core;
  const reach = Math.max(norm(sub(centre, sight.eye)) - radius, radius * 0.1, 1e-12);
  return (((2 * radius) / reach) * sight.focal) / Math.sqrt(Math.max(1, tile.points));
}

/** Whether the view needs the children of `tile`, whose points lie in `extent`: while its
 * points lie farther apart on screen than `limit` pixels. */
const refines = (tile: Tile, sight: Sight, extent: Extent, limit: number): boolean =>
  sight.inView(extent.centre, extent.radius) && spacing(tile, sight, extent) > limit;

/** A release's tiles as a tree: worked out once, read on every change of view. */
export interface Tree<T extends Tile> {
  tiles: readonly T[];
  roots: readonly T[];
  children: ReadonlyMap<string, readonly T[]>;
}

export function treeOf<T extends Tile>(tiles: readonly T[]): Tree<T> {
  const children = new Map<string, T[]>();
  for (const t of tiles) {
    if (t.parent === undefined) continue;
    const siblings = children.get(t.parent);
    if (siblings === undefined) children.set(t.parent, [t]);
    else siblings.push(t);
  }
  return { tiles, roots: tiles.filter((t) => t.parent === undefined), children };
}

/** What a catalogue holds, as the plan reads it. */
export interface Holding {
  /** Tiles settled: drawn, or failed and given up. */
  settled: ReadonlySet<string>;
  /** The tiles drawn, which may be dropped, with where their points lie where it is known. */
  drawn: ReadonlyMap<string, { readonly extent?: Extent }>;
  pending: ReadonlySet<string>;
  /** The points of the tiles drawn and on their way. */
  points: number;
}

/** Whether `holding` holds the tile `id`: drawn, or on its way. */
const holds = (holding: Holding, id: string): boolean => holding.drawn.has(id) || holding.pending.has(id);

/** The tiles to fetch next and the drawn tiles to drop to make room for them. */
export interface Plan<T> {
  fetch: T[];
  drop: T[];
}

/** A wanted tile's children in view, wanted together. */
interface Family<T extends Tile> {
  parent: T;
  tiles: readonly T[];
  points: number;
  /** How much the view needs them: the largest of them on screen, times `HELD_PREFERENCE` where
   * the family holds a tile. */
  size: number;
}

/** Inserts `family` into `frontier`, which is kept smallest first. */
function insert<T extends Tile>(frontier: Family<T>[], family: Family<T>): void {
  let lo = 0;
  let hi = frontier.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((frontier[mid]?.size ?? 0) < family.size) lo = mid + 1;
    else hi = mid;
  }
  frontier.splice(lo, 0, family);
}

/** The families the view wants within `budget`, the most needed first, after the roots. It
 * depends on what the view sees, where read tiles' points lie and which families are held: held
 * children are kept down to `KEEP_SPACING`, out to `HELD_VIEW_MARGIN` beyond the view, and a
 * held family counts as `HELD_PREFERENCE` times its size, so that what a view holds once it has fetched what it wants is what it wants, and a
 * view that does not change settles. */
function wanted<T extends Tile>(tree: Tree<T>, holding: Holding, sight: Sight, budget: number): Family<T>[] {
  let points = tree.roots.reduce((sum, t) => sum + t.points, 0);
  const frontier: Family<T>[] = [];
  const isHeld = (t: T) => holds(holding, t.id);
  // A held tile counts as in view out to `HELD_VIEW_MARGIN` beyond the view's edge.
  const inView = (t: T) =>
    sight.inView(t.centre, radiusOf(t) + (isHeld(t) ? norm(sub(t.centre, sight.eye)) * Math.tan(HELD_VIEW_MARGIN) : 0));
  const open = (tile: T) => {
    const tiles = (tree.children.get(tile.id) ?? []).filter(inView);
    if (tiles.length === 0) return;
    const held = tiles.some(isHeld);
    if (!refines(tile, sight, holding.drawn.get(tile.id)?.extent ?? cubeExtent(tile), held ? KEEP_SPACING : REFINE_SPACING)) return;
    const largest = Math.max(...tiles.map((c) => apparentSize(c, sight.eye)));
    const size = held ? largest * HELD_PREFERENCE : largest;
    insert(frontier, { parent: tile, tiles, points: tiles.reduce((sum, c) => sum + c.points, 0), size });
  };
  for (const root of tree.roots) open(root);
  const chosen: Family<T>[] = [];
  for (let next = frontier.pop(); next !== undefined; next = frontier.pop()) {
    if (points + next.points > budget) continue;
    points += next.points;
    chosen.push(next);
    for (const tile of next.tiles) open(tile);
  }
  return chosen;
}

/** Whether the tile `id` is neither settled nor on its way. */
const isOpen = (holding: Holding, id: string): boolean => !holding.settled.has(id) && !holding.pending.has(id);

/** What to fetch with level of detail off: `count` of the tiles not yet held, the coarser levels
 * first, whatever the view and the budget, and nothing to drop. */
export function planEvery<T extends Tile>(tree: Tree<T>, holding: Holding, count: number): Plan<T> {
  const open = tree.tiles.filter((t) => isOpen(holding, t.id));
  return { fetch: open.sort((a, b) => a.address.length - b.address.length).slice(0, count), drop: [] };
}

/** What to fetch and drop for `sight`: every root first; then each wanted family whose parent is
 * settled, the most needed first, its tiles not yet held all at once once the budget has room,
 * made by dropping held tiles the view does not want. At most `count` families start at once.
 * Held points beyond the budget, as level of detail turned on again leaves them, are dropped
 * the same way until they fit. */
export function plan<T extends Tile>(
  tree: Tree<T>,
  holding: Holding,
  sight: Sight,
  count: number,
  budget = POINT_BUDGET,
): Plan<T> {
  const open = (t: T) => isOpen(holding, t.id);
  const roots = tree.roots.filter(open);
  if (roots.length > 0) return { fetch: roots.slice(0, count), drop: [] };
  const families = wanted(tree, holding, sight, budget);
  const want = new Set(families.flatMap((f) => f.tiles.map((t) => t.id)));
  const room = new Room(tree, holding, sight, want, budget);
  const fetch: T[] = [];
  let started = 0;
  for (const family of families) {
    if (started >= count) break;
    if (!holding.settled.has(family.parent.id)) continue;
    const tiles = family.tiles.filter(open);
    if (tiles.length === 0 || !room.make(tiles.reduce((sum, t) => sum + t.points, 0))) continue;
    fetch.push(...tiles.sort((a, b) => apparentSize(b, sight.eye) - apparentSize(a, sight.eye)));
    started += 1;
  }
  room.trim();
  return { fetch, drop: room.drop };
}

/** Room in the budget, made by dropping held tiles the view does not want: out of view first,
 * then the smallest on screen, leaves first, a parent once its children are gone. */
class Room<T extends Tile> {
  readonly drop: T[] = [];
  private readonly dropped = new Set<string>();
  private spare: T[] = [];
  private points: number;

  constructor(
    private readonly tree: Tree<T>,
    private readonly holding: Holding,
    private readonly sight: Sight,
    private readonly want: ReadonlySet<string>,
    private readonly budget: number,
  ) {
    this.points = holding.points;
  }

  /** Makes room for `points` more, dropping what it must; nothing is dropped where they cannot fit. */
  make(points: number): boolean {
    const before = { drops: this.drop.length, points: this.points, spare: [...this.spare] };
    while (this.points + points > this.budget) {
      const next = this.next();
      if (next === undefined) {
        for (const t of this.drop.splice(before.drops)) this.dropped.delete(t.id);
        this.points = before.points;
        this.spare = before.spare;
        return false;
      }
      this.take(next);
    }
    this.points += points;
    return true;
  }

  /** Drops what the view does not want until the points held fit the budget, or nothing it
   * may drop is left. */
  trim(): void {
    while (this.points > this.budget) {
      const next = this.next();
      if (next === undefined) return;
      this.take(next);
    }
  }

  private take(tile: T): void {
    this.drop.push(tile);
    this.dropped.add(tile.id);
    this.points -= tile.points;
  }

  /** The next tile to drop: the least needed leaf of what is held and not wanted. */
  private next(): T | undefined {
    if (this.spare.length === 0) this.spare = this.leaves();
    return this.spare.shift();
  }

  /** The held tiles not wanted, nor a root, nor with a child drawn or on its way, least needed first. */
  private leaves(): T[] {
    const { holding, dropped, sight } = this;
    const held = (id: string) => !dropped.has(id) && holds(holding, id);
    const parents = new Set<string>();
    for (const t of this.tree.tiles) if (t.parent !== undefined && held(t.id)) parents.add(t.parent);
    return this.tree.tiles
      .filter((t) => t.parent !== undefined && holding.drawn.has(t.id) && !dropped.has(t.id) && !this.want.has(t.id) && !parents.has(t.id))
      .map((tile) => {
        const extent = holding.drawn.get(tile.id)?.extent ?? cubeExtent(tile);
        return { tile, seen: sight.inView(extent.centre, extent.radius) ? 1 : 0, size: apparentSize(tile, sight.eye) };
      })
      .sort((a, b) => a.seen - b.seen || a.size - b.size)
      .map((l) => l.tile);
  }
}
