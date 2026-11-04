// The tiles the explorer holds for one release (draw-the-catalogues): which to fetch and drop as
// the view moves, how many points each kind of value has, which failed, and whose sources are
// drawn.

import type { ManifestSource } from "univ-tile-decoder";

import type { ReadOptions } from "../api/client.ts";
import type { Sight } from "../scene/frustum.ts";
import { plan } from "./lod.ts";
import { type Batch, type Domain, type DomainTile, TileFailed, fetchBatch } from "./tiles.ts";

/** Tiles fetched at once. */
const PARALLEL = 6;
/** Reads of one tile that may fail for a reason other than its check before it is given up. */
const ATTEMPTS = 3;
/** The wait before a tile whose read failed is asked for again, milliseconds. */
const RETRY_MS = 2000;

export interface CatalogueEvents {
  /** A tile's points, to draw. */
  onBatch: (batch: Batch) => void;
  /** A tile's points, drawn before, to draw no more. */
  onDrop: (batch: Batch) => void;
  /** The counts or credits changed. */
  onChange: () => void;
}

/** How the catalogue reads its tiles, and the most points it holds (by default `POINT_BUDGET`). */
export type CatalogueOptions = ReadOptions & { budget?: number };

/** A source of the drawn points, with the domain whose manifest credits it. */
export type Credit = ManifestSource & { domain: string };

/** The tiles held for `release`, fetched and dropped as `update` is told what the eye sees. */
export class Catalogue {
  readonly release: number;
  private readonly domains: ReadonlyMap<string, Domain>;
  private readonly tiles: readonly DomainTile[];
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
  private sight: Sight | undefined;
  /** Points drawn of each kind of value, by its index in `KINDS`. */
  readonly kinds = [0, 0, 0, 0];
  /** The domains with a point drawn. */
  private readonly drawnDomains = new Set<string>();
  /** Tiles that failed their SHA-256 or did not decode. */
  failed = 0;
  /** Tiles that could not be read, after `ATTEMPTS` tries. */
  unread = 0;

  constructor(release: number, domains: readonly Domain[], events: CatalogueEvents, options: CatalogueOptions = {}) {
    this.release = release;
    this.domains = new Map(domains.map((d) => [d.name, d]));
    this.tiles = domains.flatMap((d) => d.tiles);
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
    const holding = {
      settled: this.settled,
      drawn: new Set(this.drawn.keys()),
      pending,
      points: this.points,
      extent: (id: string) => this.drawn.get(id)?.extent,
    };
    const next = plan(this.tiles, holding, sight, room, this.options.budget);
    for (const tile of next.drop) this.drop(tile);
    for (const tile of next.fetch) {
      const domain = this.domains.get(tile.domain);
      if (domain !== undefined) this.fetch(domain, tile);
    }
    if (next.drop.length > 0) this.events.onChange();
  }

  private fetch(domain: Domain, tile: DomainTile): void {
    this.pending.add(tile.id);
    this.points += tile.points;
    void fetchBatch(domain, tile, this.release, this.options)
      .then((batch) => {
        this.settled.add(tile.id);
        this.drawn.set(tile.id, batch);
        this.count(domain, batch);
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

  /** Lets go of a drawn tile's points; it is fetched again when the view needs it. */
  private drop(tile: DomainTile): void {
    const batch = this.drawn.get(tile.id);
    if (batch === undefined) return;
    this.drawn.delete(tile.id);
    this.settled.delete(tile.id);
    this.points -= tile.points;
    this.tally(batch, -1);
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

  /** Counts `batch`'s points into the kinds of value, or out of them where `by` is −1. */
  private tally(batch: Batch, by: 1 | -1): void {
    for (const kind of batch.kinds) this.kinds[kind] = (this.kinds[kind] ?? 0) + by;
  }

  private count(domain: Domain, batch: Batch): void {
    this.tally(batch, 1);
    if (batch.kinds.length > 0) this.drawnDomains.add(domain.name);
  }

  /** The sources of every domain with a point drawn, as their manifests credit them. */
  credits(): Credit[] {
    return [...this.domains.values()]
      .filter((d) => this.drawnDomains.has(d.name))
      .flatMap((d) => d.manifest.sources.map((s) => ({ ...s, domain: d.name })));
  }
}
