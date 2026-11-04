// The tiles the explorer holds for one release (draw-the-catalogues): which to fetch as the view
// moves, how many points each kind of value has, which failed, and whose sources are drawn.

import type { ManifestSource } from "univ-tile-decoder";

import type { ReadOptions } from "../api/client.ts";
import type { Vec3 } from "../scene/vec.ts";
import { nextTiles } from "./lod.ts";
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
  /** The counts or credits changed. */
  onChange: () => void;
}

/** A source of the drawn points, with the domain whose manifest credits it. */
export type Credit = ManifestSource & { domain: string };

/** The tiles held for `release`, fetched as `update` is told where the eye is. */
export class Catalogue {
  readonly release: number;
  private readonly domains: ReadonlyMap<string, Domain>;
  private readonly tiles: readonly DomainTile[];
  private readonly events: CatalogueEvents;
  private readonly options: ReadOptions;
  /** Tiles drawn, failed or given up: never asked for again. */
  private readonly held = new Set<string>();
  private readonly pending = new Set<string>();
  /** Tiles whose read failed, waiting out `RETRY_MS` before they are asked for again. */
  private readonly resting = new Set<string>();
  /** Reads of each tile that failed for a reason other than its check. */
  private readonly attempts = new Map<string, number>();
  private heldPoints = 0;
  private eye: Vec3 = [0, 0, 0];
  /** Points drawn of each kind of value, by its index in `KINDS`. */
  readonly kinds = [0, 0, 0, 0];
  /** The domains with a point drawn. */
  private readonly drawnDomains = new Set<string>();
  /** Tiles that failed their SHA-256 or did not decode. */
  failed = 0;
  /** Tiles that could not be read, after `ATTEMPTS` tries. */
  unread = 0;

  constructor(release: number, domains: readonly Domain[], events: CatalogueEvents, options: ReadOptions = {}) {
    this.release = release;
    this.domains = new Map(domains.map((d) => [d.name, d]));
    this.tiles = domains.flatMap((d) => d.tiles);
    this.events = events;
    this.options = options;
  }

  private get aborted(): boolean {
    return this.options.signal?.aborted === true;
  }

  /** Fetches the tiles the view from `eye` needs next. */
  update(eye: Vec3): void {
    this.eye = eye;
    const room = PARALLEL - this.pending.size;
    if (room <= 0 || this.held.size + this.pending.size === this.tiles.length) return;
    const skipped = this.resting.size === 0 ? this.pending : new Set([...this.pending, ...this.resting]);
    for (const tile of nextTiles(this.tiles, this.held, skipped, this.heldPoints, eye, room)) {
      const domain = this.domains.get(tile.domain);
      if (domain !== undefined) this.fetch(domain, tile);
    }
  }

  private fetch(domain: Domain, tile: DomainTile): void {
    this.pending.add(tile.id);
    void fetchBatch(domain, tile, this.release, this.options)
      .then((batch) => {
        this.hold(tile);
        this.count(domain, batch);
        this.events.onBatch(batch);
      })
      .catch((error: unknown) => {
        if (!this.aborted) this.failedRead(tile, error);
      })
      .finally(() => {
        this.pending.delete(tile.id);
        this.events.onChange();
        if (!this.aborted) this.update(this.eye);
      });
  }

  private hold(tile: DomainTile): void {
    this.held.add(tile.id);
    this.heldPoints += tile.points;
  }

  /** A tile that failed its check is given up at once; one not read is tried again later. */
  private failedRead(tile: DomainTile, error: unknown): void {
    if (error instanceof TileFailed) {
      this.held.add(tile.id);
      this.failed += 1;
      return;
    }
    const attempts = (this.attempts.get(tile.id) ?? 0) + 1;
    this.attempts.set(tile.id, attempts);
    if (attempts >= ATTEMPTS) {
      this.held.add(tile.id);
      this.unread += 1;
      return;
    }
    this.resting.add(tile.id);
    setTimeout(() => {
      this.resting.delete(tile.id);
      if (!this.aborted) this.update(this.eye);
    }, RETRY_MS);
  }

  private count(domain: Domain, batch: Batch): void {
    for (const kind of batch.kinds) this.kinds[kind] = (this.kinds[kind] ?? 0) + 1;
    if (batch.kinds.length > 0) this.drawnDomains.add(domain.name);
  }

  /** The sources of every domain with a point drawn, as their manifests credit them. */
  credits(): Credit[] {
    return [...this.domains.values()]
      .filter((d) => this.drawnDomains.has(d.name))
      .flatMap((d) => d.manifest.sources.map((s) => ({ ...s, domain: d.name })));
  }
}
