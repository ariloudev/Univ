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
const radiusOf = (tile: Tile): number => tile.edge * (Math.sqrt(3) / 2);

/** Where a tile's points lie: the sphere about their box, which holds every one, and within it
 * the radius that holds most of them. A domain's cubes are far larger than its data (the cosmic
 * web's root is 16 Gpc across, its surveys a few), and a few far objects stretch a box (the
 * root's reach 6 Gpc while 90 % lie within 1.3 Gpc), so once a tile's points are read their
 * sphere decides what is in view and their core how far apart they lie on screen; before, the
 * tile's cube decides both. */
export interface Extent {
  centre: Vec3;
  radius: number;
  /** The sphere about the points' mean that holds `CORE` of them. */
  core: { centre: Vec3; radius: number };
}

/** The share of a tile's points its core holds. */
const CORE = 0.9;

/** The sphere about a tile's cube. */
function cubeExtent(tile: Tile): Extent {
  const sphere = { centre: tile.centre, radius: radiusOf(tile) };
  return { ...sphere, core: sphere };
}

/** The sphere about the box of `offsets` (x, y, z triples) from `centre`. */
export function extentOf(offsets: Float32Array, centre: Vec3): Extent {
  if (offsets.length < 3) return { centre, radius: 0, core: { centre, radius: 0 } };
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < offsets.length; i += 1) {
    const v = offsets[i] ?? 0;
    const axis = i % 3;
    if (v < (lo[axis] ?? 0)) lo[axis] = v;
    if (v > (hi[axis] ?? 0)) hi[axis] = v;
  }
  const mid = (a: number) => ((lo[a] ?? 0) + (hi[a] ?? 0)) / 2;
  const half = (a: number) => ((hi[a] ?? 0) - (lo[a] ?? 0)) / 2;
  return {
    centre: [centre[0] + mid(0), centre[1] + mid(1), centre[2] + mid(2)],
    radius: Math.hypot(half(0), half(1), half(2)),
    core: coreOf(offsets, centre),
  };
}

/** The sphere about the mean of `offsets` from `centre` that holds `CORE` of them. */
function coreOf(offsets: Float32Array, centre: Vec3): { centre: Vec3; radius: number } {
  const n = offsets.length / 3;
  const mean = [0, 1, 2].map((a) => {
    let sum = 0;
    for (let i = a; i < offsets.length; i += 3) sum += offsets[i] ?? 0;
    return sum / n;
  });
  const [mx = 0, my = 0, mz = 0] = mean;
  const reach = new Float64Array(n);
  for (let i = 0; i < n; i += 1) {
    reach[i] = Math.hypot((offsets[i * 3] ?? 0) - mx, (offsets[i * 3 + 1] ?? 0) - my, (offsets[i * 3 + 2] ?? 0) - mz);
  }
  reach.sort();
  return { centre: [centre[0] + mx, centre[1] + my, centre[2] + mz], radius: reach[Math.ceil(CORE * n) - 1] ?? 0 };
}

/** How far apart a tile's points lie on screen, pixels: its points spread over the square their
 * core covers on screen, every point of its depth landing on that square. */
export function spacing(tile: Tile, sight: Sight, extent = cubeExtent(tile)): number {
  const { centre, radius } = extent.core;
  const reach = Math.max(norm(sub(centre, sight.eye)) - radius, radius * 0.1, 1e-12);
  return (((2 * radius) / reach) * sight.focal) / Math.sqrt(Math.max(1, tile.points));
}

/** Whether the view needs the children of `tile`, whose points lie in `extent`. */
const refines = (tile: Tile, sight: Sight, extent = cubeExtent(tile)): boolean =>
  sight.inView(extent.centre, extent.radius) && spacing(tile, sight, extent) > REFINE_SPACING;

/** What a catalogue holds, as the plan reads it. */
export interface Holding {
  /** Tiles settled: drawn, or failed and given up. */
  settled: ReadonlySet<string>;
  /** Tiles drawn, which may be dropped. */
  drawn: ReadonlySet<string>;
  pending: ReadonlySet<string>;
  /** The points of the tiles drawn and on their way. */
  points: number;
  /** Where a drawn tile's points lie, where it is known. */
  extent?: (id: string) => Extent | undefined;
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
const byNeed = <T>(a: { need: [number, number] }, b: { need: [number, number] }) => (less(a.need, b.need) ? -1 : less(b.need, a.need) ? 1 : 0);

/** The tiles the view wants, within `budget`: every root, then, the most needed first, the
 * children in view of each wanted tile the view refines, all of a tile's together, so that the
 * budget never leaves one octant of a region coarser than the rest. It depends on what the view
 * sees and where read tiles' points lie, not on what is held, so a view that does not change
 * settles. */
function wanted<T extends Tile>(tiles: readonly T[], holding: Holding, sight: Sight, budget: number): Set<string> {
  const children = new Map<string, T[]>();
  for (const t of tiles) {
    if (t.parent !== undefined) children.set(t.parent, [...(children.get(t.parent) ?? []), t]);
  }
  const roots = tiles.filter((t) => t.parent === undefined);
  const chosen = new Set(roots.map((t) => t.id));
  let points = roots.reduce((sum, t) => sum + t.points, 0);
  const frontier: { tiles: T[]; points: number; need: [number, number] }[] = [];
  const open = (tile: T) => {
    if (!refines(tile, sight, holding.extent?.(tile.id))) return;
    const seen = (children.get(tile.id) ?? []).filter((c) => sight.inView(c.centre, radiusOf(c)));
    if (seen.length === 0) return;
    const needs = seen.map((c) => need(c, sight)).sort((x, y) => (less(x, y) ? 1 : less(y, x) ? -1 : 0));
    frontier.push({ tiles: seen, points: seen.reduce((sum, c) => sum + c.points, 0), need: needs[0] ?? [0, 0] });
    frontier.sort(byNeed);
  };
  for (const root of roots) open(root);
  for (let next = frontier.pop(); next !== undefined; next = frontier.pop()) {
    if (points + next.points > budget) continue;
    points += next.points;
    for (const tile of next.tiles) chosen.add(tile.id);
    for (const tile of next.tiles) open(tile);
  }
  return chosen;
}

/** What to fetch and drop for `sight`: every root first; then the wanted tiles not held whose
 * parent is, the most needed first, each once the budget has room, made by dropping held tiles the
 * view does not want, out of view first, then the smallest on screen, a parent only after its
 * children. */
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
  const want = wanted(tiles, holding, sight, budget);
  const ranked = open
    .filter((t) => want.has(t.id) && t.parent !== undefined && holding.settled.has(t.parent))
    .map((tile) => ({ tile, need: need(tile, sight) }))
    .sort((a, b) => byNeed(b, a));
  const spare = droppable(tiles, holding, sight).filter((s) => !want.has(s.tile.id));
  const fetch: T[] = [];
  const drop: T[] = [];
  let points = holding.points;
  for (const { tile } of ranked) {
    if (fetch.length >= count) break;
    while (points + tile.points > budget && drop.length < spare.length) {
      const next = spare[drop.length];
      if (next === undefined) break;
      drop.push(next.tile);
      points -= next.tile.points;
    }
    if (points + tile.points > budget) break;
    points += tile.points;
    fetch.push(tile);
  }
  return { fetch, drop };
}

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
    .sort(byNeed);
}
