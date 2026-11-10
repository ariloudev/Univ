// The tiles the explorer holds for one release (draw-the-catalogues): which to fetch and drop as
// the view moves, how many points each kind of value has, which failed, and whose sources are
// drawn.

import type { ManifestSource } from "univ-tile-decoder";

import type { ReadOptions } from "../api/client.ts";
import type { Sight } from "../scene/frustum.ts";
import {
  type Detail,
  type DrawnDetail,
  EVERYTHING,
  POINT_BUDGET,
  type Tree,
  detailOf,
  drawnShare,
  isWithin,
  neededAt,
  onsetsOf,
  plan,
  planEvery,
  rankOf,
  treeOf,
} from "./lod.ts";
import type { Rgba } from "../scene/drawing.ts";
import { KINDS, KIND_INDEX } from "../scene/kinds.ts";
import { type Masks, NO_MASKS, isPointShown } from "../scene/masks.ts";
import { TRACERS_DOMAIN, tracerColour } from "../scene/tracers.ts";
import { type Volume, insideOf } from "../scene/volume.ts";
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

/** A source of the Galaxy's tracers in the key: its colour and its points drawn. */
export interface TracerLine {
  id: string;
  title: string;
  colour: Rgba;
  points: number;
}

/** A batch's points counted by kind of value, source and whether coloured, so that what is
 * turned off is counted out by cell rather than point by point; and, where the view is culled
 * to a volume, which of them lie inside it, and those counted alone. */
interface Histogram {
  /** The sources of the batch's domain: one more than the highest index of its manifest. */
  sources: number;
  /** Every point, by cell `(kind × sources + source) × 2 + coloured`. */
  all: Uint32Array;
  /** The points counted: every one, or those inside the volume the view is culled to. */
  cells: Uint32Array;
  /** Which points lie inside the volume the view is culled to, 1 inside; none where it is not. */
  inside: Uint8Array | undefined;
  /** Whether the view is culled and no point lies inside. */
  empty: boolean;
}

/** `batch`'s points counted by cell, its domain having `sources` sources: only those `inside`
 * marks, where it marks any. */
function cellsOf(batch: Batch, sources: number, inside?: Uint8Array): Uint32Array {
  const cells = new Uint32Array(KINDS.length * sources * 2);
  const bands = batch.stars?.bands;
  for (let i = 0; i < batch.kinds.length; i += 1) {
    if (inside !== undefined && inside[i] === 0) continue;
    const at = ((batch.kinds[i] ?? 0) * sources + (batch.sources[i] ?? 0)) * 2 + ((bands?.[i] ?? 0) > 0 ? 1 : 0);
    cells[at] = (cells[at] ?? 0) + 1;
  }
  return cells;
}

/** The tiles held for `release`, fetched and dropped as `update` is told what the eye sees. */
export class Catalogue implements DrawnDetail {
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
  /** Whether the tiles held follow the view within the budget; off, every tile is fetched. */
  private levelOfDetail = true;
  /** Each domain's detail as the view needs it at `viewedFocal`, by domain. */
  private readonly viewedDomains = new Map<string, Detail>();
  private viewedFocal = 0;
  /** Points drawn outside the stars domain of each kind of value, by its index in `KINDS`. */
  readonly kinds = [0, 0, 0, 0];
  /** Stars drawn in the colour of their colour index, and in grey for want of one; and of them,
   * those whose preferred distance is inferred, drawn half as opaque. */
  readonly stars = { coloured: 0, grey: 0, inferred: 0 };
  /** The Galaxy's tracers drawn, by their source's index in the domain's manifest. */
  private readonly tracerPoints: number[] = [];
  /** The Galaxy's tracers drawn of each kind of value, by its index in `KINDS`. */
  readonly tracerKinds = [0, 0, 0, 0];
  /** The points counted of each domain. */
  private readonly domainPoints = new Map<string, number>();
  /** Each drawn tile's points by cell, and which lie inside the volume the view is culled to, by
   * tile id. */
  private readonly histograms = new Map<string, Histogram>();
  /** What is turned off: points it hides are not counted, nor their sources credited. */
  private masks: Masks = NO_MASKS;
  /** The volume the view is culled to, if it is (see-the-structures). */
  private cull: Volume | undefined;
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
    // With no room to fetch, only held points beyond the budget, as level of detail turned on
    // again leaves them, are worth a plan: it drops what the view does not want.
    const room = Math.max(0, PARALLEL - this.pending.size);
    if (room === 0 && this.points <= (this.options.budget ?? POINT_BUDGET)) return;
    const pending = this.resting.size === 0 ? this.pending : new Set([...this.pending, ...this.resting]);
    const holding = { settled: this.settled, drawn: this.drawn, pending, points: this.points };
    const within = this.cull?.bounds;
    const next = this.levelOfDetail
      ? plan(this.tree, holding, sight, room, this.options.budget, within)
      : planEvery(this.tree, holding, room, within);
    for (const tile of next.drop) this.drop(tile);
    let revived = false;
    for (const tile of next.fetch) {
      if (this.revive(tile)) {
        revived = true;
        continue;
      }
      const domain = this.domains.get(tile.domain);
      if (domain !== undefined) this.fetch(domain, tile);
    }
    if (next.drop.length > 0) this.letGoBeyondAllowance();
    if (this.pending.size === 0) this.letGoKept();
    // A revived tile ends no fetch, which is what asks for the next round: the plan, which could
    // not refine it while it was not held, is asked again.
    if (revived) queueMicrotask(() => this.update(sight));
  }

  /** `domain`'s detail as the view last seen needs it, worked out once for each focal length:
   * none with level of detail off or before the view is known. */
  private viewed(domain: string): Detail | undefined {
    const levels = this.tree.levels.get(domain);
    if (!this.levelOfDetail || this.sight === undefined || levels === undefined) return undefined;
    if (this.sight.focal !== this.viewedFocal) {
      this.viewedDomains.clear();
      this.viewedFocal = this.sight.focal;
    }
    let viewed = this.viewedDomains.get(domain);
    if (viewed === undefined) {
      viewed = detailOf(levels, onsetsOf(levels, this.sight.focal));
      this.viewedDomains.set(domain, viewed);
    }
    return viewed;
  }

  /** The share of the tile `id`'s points, its first, that may be drawn for the view last seen:
   * every one with level of detail off, or before the view is known. */
  share(id: string): number {
    if (this.histograms.get(id)?.empty === true) return 0;
    const tile = this.tree.byId.get(id);
    const viewed = tile === undefined ? undefined : this.viewed(tile.domain);
    if (tile === undefined || viewed === undefined || this.sight === undefined) return 1;
    const read = this.drawn.get(id) ?? this.kept.get(id);
    return drawnShare(tile, viewed, this.sight.eye, read?.extent);
  }

  /** What the renderer needs to draw the points of `domain` for the view last seen; with level of
   * detail off, all of it everywhere. */
  detail(domain: string): Detail {
    return this.viewed(domain) ?? EVERYTHING;
  }

  /** Which points of `batch` are drawn for the view last seen: those whose rank lies below the
   * share of their domain the view needs at their own distance from the eye. */
  drawnPoints(batch: Batch): (index: number) => boolean {
    const inside = this.inside(batch.tile.id);
    const drawn = this.drawnByDetail(batch);
    return inside === undefined ? drawn : (i) => inside[i] === 1 && drawn(i);
  }

  /** Which points of `batch` level of detail draws for the view last seen. */
  private drawnByDetail(batch: Batch): (index: number) => boolean {
    const viewed = this.viewed(batch.tile.domain);
    const eye = this.sight?.eye;
    if (viewed === undefined || eye === undefined) return () => true;
    const { tile, offsets } = batch;
    const count = offsets.length / 3;
    return (i) => {
      // By hand, not by vec.ts, which would allocate for every point tested.
      const dx = tile.centre[0] + (offsets[i * 3] ?? 0) - eye[0];
      const dy = tile.centre[1] + (offsets[i * 3 + 1] ?? 0) - eye[1];
      const dz = tile.centre[2] + (offsets[i * 3 + 2] ?? 0) - eye[2];
      return rankOf(tile, i, count) < neededAt(viewed, Math.sqrt(dx * dx + dy * dy + dz * dz));
    };
  }

  /** Turns level of detail on or off: off, every tile is fetched, the coarser levels first, and
   * none dropped; on again, the tiles held come back within the budget. */
  setLevelOfDetail(on: boolean): void {
    if (on === this.levelOfDetail) return;
    this.levelOfDetail = on;
    this.events.onChange();
    if (this.sight !== undefined) this.update(this.sight);
  }

  private fetch(domain: Domain, tile: DomainTile): void {
    this.pending.add(tile.id);
    this.points += tile.points;
    void fetchBatch(domain, tile, this.release, this.options)
      .then((batch) => {
        this.hold(tile, batch);
        this.measure(batch);
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
      const source = Math.floor(at / 2) % sources;
      if (n === 0 || !isPointShown(this.masks, domain, source, kind)) return;
      if (domain === TRACERS_DOMAIN) this.countTracer(source, kind, by * n);
      else this.count(batch.stars !== undefined, kind, at % 2 === 1, by * n);
      this.domainPoints.set(domain, (this.domainPoints.get(domain) ?? 0) + by * n);
    });
  }

  /** Counts `n` points into the stars' colours, or into their kind of value. */
  private count(star: boolean, kind: number, coloured: boolean, n: number): void {
    if (!star) {
      this.kinds[kind] = (this.kinds[kind] ?? 0) + n;
      return;
    }
    if (coloured) this.stars.coloured += n;
    else this.stars.grey += n;
    if (kind === KIND_INDEX.inferred) this.stars.inferred += n;
  }

  /** Counts `n` tracers of `source` and `kind` (their indices). */
  private countTracer(source: number, kind: number, n: number): void {
    this.tracerPoints[source] = (this.tracerPoints[source] ?? 0) + n;
    this.tracerKinds[kind] = (this.tracerKinds[kind] ?? 0) + n;
  }

  /** Each source of the Galaxy's tracers, its colour and its points drawn. */
  tracers(): TracerLine[] {
    const sources = this.domains.get(TRACERS_DOMAIN)?.manifest.sources ?? [];
    return sources.map((s) => ({ id: s.id, title: s.title, colour: tracerColour(s.index), points: this.tracerPoints[s.index] ?? 0 }));
  }

  /** Counts again, once, every drawn point that `masks` leaves on (choose-what-is-drawn). */
  setMasks(masks: Masks): void {
    this.masks = masks;
    this.recount();
  }

  /** Counts every point held again, as `masks` and the cull leave it. */
  private recount(): void {
    this.kinds.fill(0);
    this.stars.coloured = 0;
    this.stars.grey = 0;
    this.stars.inferred = 0;
    this.tracerPoints.length = 0;
    this.tracerKinds.fill(0);
    this.domainPoints.clear();
    for (const batch of [...this.drawn.values(), ...this.kept.values()]) this.tally(batch, 1);
    this.events.onChange();
  }

  /** Which points of the tile `id` lie inside the volume the view is culled to; none where it is
   * not culled, or the tile is not held. */
  inside(id: string): Uint8Array | undefined {
    return this.histograms.get(id)?.inside;
  }

  /** Counts `batch`'s points by cell, and marks which lie inside the volume the view is culled
   * to, if it is, counting those alone: every point is counted once, when it is read. */
  private measure(batch: Batch): void {
    const id = batch.tile.id;
    const domain = this.domains.get(batch.tile.domain);
    if (domain === undefined) return;
    const sources = sourcesOf(domain);
    const all = this.histograms.get(id)?.all ?? cellsOf(batch, sources);
    const inside = this.cull === undefined ? undefined : insideOf(this.cull, batch.tile.centre, batch.offsets, batch.extent);
    const cells = inside === undefined ? all : cellsOf(batch, sources, inside);
    this.histograms.set(id, { sources, all, cells, inside, empty: inside !== undefined && !inside.includes(1) });
  }

  /** Culls the view to `volume`, or no longer where it is none (see-the-structures): the points
   * outside it are counted out, the tiles whose cubes miss its bounding box dropped, roots too,
   * and only tiles that meet it fetched. */
  setCull(volume: Volume | undefined): void {
    if (volume === this.cull) return;
    this.cull = volume;
    for (const batch of [...this.drawn.values(), ...this.kept.values()]) this.measure(batch);
    if (volume !== undefined) {
      const missing = [...this.drawn.values()].map((b) => b.tile).filter((t) => !isWithin(t, volume.bounds));
      for (const tile of missing) this.drop(tile);
      if (missing.length > 0) this.letGoBeyondAllowance();
    }
    this.recount();
    if (this.sight !== undefined) this.update(this.sight);
  }

  /** The sources of every domain with a point drawn, as their manifests credit them: all of
   * them, since a point's sky position may come from a source other than its distance's. */
  credits(): Credit[] {
    return [...this.domains.values()]
      .filter((d) => (this.domainPoints.get(d.name) ?? 0) > 0)
      .flatMap((d) => d.manifest.sources.map((s) => ({ ...s, domain: d.name })));
  }
}
