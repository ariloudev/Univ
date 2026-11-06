// The tiles the explorer holds for one release (draw-the-catalogues): which to fetch and drop as
// the view moves, how many points each kind of value has, which failed, and whose sources are
// drawn.

import type { ManifestSource } from "univ-tile-decoder";

import type { ReadOptions } from "../api/client.ts";
import type { Sight } from "../scene/frustum.ts";
import { POINT_BUDGET, type Tree, plan, treeOf } from "./lod.ts";
import { KINDS } from "../scene/kinds.ts";
import { type Masks, NO_MASKS, isPointShown } from "../scene/masks.ts";
import { type Batch, type Domain, type DomainTile, TileFailed, fetchBatch } from "./tiles.ts";

/** Tiles fetched at once. */
const PARALLEL = 6;
/** Reads of one tile that may fail for a reason other than its check before it is given up. */
const ATTEMPTS = 3;
/** The wait before a tile whose read failed is asked for again, milliseconds. */
const RETRY_MS = 2000;
/** The share of the budget that dropped tiles may hold beyond it while they are kept drawn. */
const KEEP_SHARE = 0.25;
/** The most points of dropped tiles kept drawn beyond the default budget: a quarter of it. */
export const KEEP_ALLOWANCE = POINT_BUDGET * KEEP_SHARE;

export interface CatalogueEvents {
  /** A tile's points, to draw. */
  onBatch: (batch: Batch) => void;
  /** A tile's points, drawn before, to draw no more. */
  onDrop: (batch: Batch) => void;
  /** The counts or credits changed. */
  onChange: () => void;
}

/** How the catalogue reads its tiles, the most points it holds (by default `POINT_BUDGET`), and
 * the most points of dropped tiles it keeps drawn (by default a quarter of its budget). */
export type CatalogueOptions = ReadOptions & { budget?: number; keep?: number };

/** A source of the drawn points, with the domain whose manifest credits it. */
export type Credit = ManifestSource & { domain: string };

/** The sources `domain`'s points may name: one more than its manifest's highest index. */
const sourcesOf = (domain: Domain): number => Math.max(0, ...domain.manifest.sources.map((s) => s.index)) + 1;

/** A batch's points counted by kind of value, source and whether coloured, so that what is
 * turned off is counted out by cell rather than point by point. */
interface Histogram {
  /** The sources of the batch's domain: one more than the highest index of its manifest. */
  sources: number;
  /** Cell `(kind × sources + source) × 2 + coloured`. */
  cells: Uint32Array;
}

/** `batch`'s points counted by cell, its domain having `sources` sources. */
function histogramOf(batch: Batch, sources: number): Histogram {
  const cells = new Uint32Array(KINDS.length * sources * 2);
  const bands = batch.stars?.bands;
  for (let i = 0; i < batch.kinds.length; i += 1) {
    const at = ((batch.kinds[i] ?? 0) * sources + (batch.sources[i] ?? 0)) * 2 + ((bands?.[i] ?? 0) > 0 ? 1 : 0);
    cells[at] = (cells[at] ?? 0) + 1;
  }
  return { sources, cells };
}

/** The tiles held for `release`, fetched and dropped as `update` is told what the eye sees. */
export class Catalogue {
  readonly release: number;
  private readonly domains: ReadonlyMap<string, Domain>;
  private readonly tree: Tree<DomainTile>;
  private readonly events: CatalogueEvents;
  private readonly options: CatalogueOptions;
  /** Tiles drawn, failed or given up; only a drawn tile is dropped and asked for again. */
  private readonly settled = new Set<string>();
  /** The tiles drawn, with their points. */
  private readonly drawn = new Map<string, Batch>();
  private readonly pending = new Set<string>();
  /** Tiles whose read failed, waiting out `RETRY_MS` before they are asked for again. */
  private readonly resting = new Set<string>();
  /** Reads of each tile that failed for a reason other than its check. */
  private readonly attempts = new Map<string, number>();
  /** The points of the tiles drawn and on their way. */
  private points = 0;
  /** Tiles dropped to make room and kept drawn, outside the budget, until no tile is on its
   * way: oldest first. */
  private readonly kept = new Map<string, Batch>();
  private keptPoints = 0;
  private sight: Sight | undefined;
  /** Points drawn outside the stars domain of each kind of value, by its index in `KINDS`. */
  readonly kinds = [0, 0, 0, 0];
  /** Stars drawn in the colour of their colour index, and in grey for want of one. */
  readonly stars = { coloured: 0, grey: 0 };
  /** The points counted of each domain. */
  private readonly domainPoints = new Map<string, number>();
  /** Each drawn tile's points by cell, by tile id. */
  private readonly histograms = new Map<string, Histogram>();
  /** What is turned off: points it hides are not counted, nor their sources credited. */
  private masks: Masks = NO_MASKS;
  /** Tiles that failed their SHA-256 or did not decode. */
  failed = 0;
  /** Tiles that could not be read, after `ATTEMPTS` tries. */
  unread = 0;

  constructor(release: number, domains: readonly Domain[], events: CatalogueEvents, options: CatalogueOptions = {}) {
    this.release = release;
    this.domains = new Map(domains.map((d) => [d.name, d]));
    this.tree = treeOf(domains.flatMap((d) => d.tiles));
    this.events = events;
    this.options = options;
  }

  /** The domain named `name`. */
  domain(name: string): Domain | undefined {
    return this.domains.get(name);
  }

  private get aborted(): boolean {
    return this.options.signal?.aborted === true;
  }

  /** Fetches the tiles `sight` needs next, dropping those it needs least to make room. */
  update(sight: Sight): void {
    this.sight = sight;
    const room = PARALLEL - this.pending.size;
    if (room <= 0) return;
    const pending = this.resting.size === 0 ? this.pending : new Set([...this.pending, ...this.resting]);
    const holding = { settled: this.settled, drawn: this.drawn, pending, points: this.points };
    const next = plan(this.tree, holding, sight, room, this.options.budget);
    for (const tile of next.drop) this.drop(tile);
    for (const tile of next.fetch) {
      if (this.revive(tile)) continue;
      const domain = this.domains.get(tile.domain);
      if (domain !== undefined) this.fetch(domain, tile);
    }
    if (next.drop.length > 0) this.letGoBeyondAllowance();
    if (this.pending.size === 0) this.letGoKept();
  }

  private fetch(domain: Domain, tile: DomainTile): void {
    this.pending.add(tile.id);
    this.points += tile.points;
    void fetchBatch(domain, tile, this.release, this.options)
      .then((batch) => {
        this.hold(tile, batch);
        this.histograms.set(tile.id, histogramOf(batch, sourcesOf(domain)));
        this.tally(batch, 1);
        this.events.onBatch(batch);
      })
      .catch((error: unknown) => {
        this.points -= tile.points;
        if (!this.aborted) this.failedRead(tile, error);
      })
      .finally(() => {
        this.pending.delete(tile.id);
        this.events.onChange();
        if (!this.aborted && this.sight !== undefined) this.update(this.sight);
      });
  }

  /** Counts a drawn tile in the budget. */
  private hold(tile: DomainTile, batch: Batch): void {
    this.settled.add(tile.id);
    this.drawn.set(tile.id, batch);
  }

  /** Takes a drawn tile out of the budget, keeping it drawn, counted and credited until no tile
   * is on its way; it is fetched again when the view needs it. */
  private drop(tile: DomainTile): void {
    const batch = this.drawn.get(tile.id);
    if (batch === undefined) return;
    this.drawn.delete(tile.id);
    this.settled.delete(tile.id);
    this.points -= tile.points;
    this.kept.set(tile.id, batch);
    this.keptPoints += tile.points;
  }

  /** Draws on a kept tile the view wants again, without reading it again. Whether it was kept. */
  private revive(tile: DomainTile): boolean {
    const batch = this.kept.get(tile.id);
    if (batch === undefined) return false;
    this.kept.delete(tile.id);
    this.keptPoints -= tile.points;
    this.hold(tile, batch);
    this.points += tile.points;
    return true;
  }

  /** Lets go of every kept tile: nothing is on its way that they stand in for. */
  private letGoKept(): void {
    if (this.kept.size === 0) return;
    for (const id of [...this.kept.keys()]) this.letGo(id);
    this.events.onChange();
  }

  /** Lets go of kept tiles, the longest kept first, until their points fit the allowance. */
  private letGoBeyondAllowance(): void {
    const allowance = this.options.keep ?? (this.options.budget ?? POINT_BUDGET) * KEEP_SHARE;
    let changed = false;
    for (const id of this.kept.keys()) {
      if (this.keptPoints <= allowance) break;
      this.letGo(id);
      changed = true;
    }
    if (changed) this.events.onChange();
  }

  /** Draws a kept tile no more, and counts its points out. */
  private letGo(id: string): void {
    const batch = this.kept.get(id);
    if (batch === undefined) return;
    this.kept.delete(id);
    this.keptPoints -= batch.tile.points;
    this.tally(batch, -1);
    this.histograms.delete(id);
    this.events.onDrop(batch);
  }

  /** A tile that failed its check is given up at once; one not read is tried again later. */
  private failedRead(tile: DomainTile, error: unknown): void {
    if (error instanceof TileFailed) {
      this.settled.add(tile.id);
      this.failed += 1;
      return;
    }
    const attempts = (this.attempts.get(tile.id) ?? 0) + 1;
    this.attempts.set(tile.id, attempts);
    if (attempts >= ATTEMPTS) {
      this.settled.add(tile.id);
      this.unread += 1;
      return;
    }
    this.resting.add(tile.id);
    setTimeout(() => {
      this.resting.delete(tile.id);
      if (!this.aborted && this.sight !== undefined) this.update(this.sight);
    }, RETRY_MS);
  }

  /** Counts `batch`'s points that are drawn into the stars' colours or the kinds of value, and
   * into its domain, or out of them where `by` is −1. */
  private tally(batch: Batch, by: 1 | -1): void {
    const histogram = this.histograms.get(batch.tile.id);
    if (histogram === undefined) return;
    const domain = batch.tile.domain;
    const { sources, cells } = histogram;
    cells.forEach((n, at) => {
      const kind = Math.floor(at / 2 / sources);
      if (n === 0 || !isPointShown(this.masks, domain, Math.floor(at / 2) % sources, kind)) return;
      this.count(batch.stars !== undefined, kind, at % 2 === 1, by * n);
      this.domainPoints.set(domain, (this.domainPoints.get(domain) ?? 0) + by * n);
    });
  }

  /** Counts `n` points into the stars' colours, or into their kind of value. */
  private count(star: boolean, kind: number, coloured: boolean, n: number): void {
    if (!star) this.kinds[kind] = (this.kinds[kind] ?? 0) + n;
    else if (coloured) this.stars.coloured += n;
    else this.stars.grey += n;
  }

  /** Counts again, once, every drawn point that `masks` leaves on (choose-what-is-drawn). */
  setMasks(masks: Masks): void {
    this.masks = masks;
    this.kinds.fill(0);
    this.stars.coloured = 0;
    this.stars.grey = 0;
    this.domainPoints.clear();
    for (const batch of [...this.drawn.values(), ...this.kept.values()]) this.tally(batch, 1);
    this.events.onChange();
  }

  /** The sources of every domain with a point drawn, as their manifests credit them: all of
   * them, since a point's sky position may come from a source other than its distance's. */
  credits(): Credit[] {
    return [...this.domains.values()]
      .filter((d) => (this.domainPoints.get(d.name) ?? 0) > 0)
      .flatMap((d) => d.manifest.sources.map((s) => ({ ...s, domain: d.name })));
  }
}
