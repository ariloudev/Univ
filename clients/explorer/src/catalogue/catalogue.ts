// The tiles the explorer holds for one release (draw-the-catalogues): which to fetch as the view
// moves, how many points each kind of value has, which failed, and whose sources are drawn.

import type { ReadOptions } from "../api/client.ts";
import type { Vec3 } from "../scene/vec.ts";
import { nextTiles } from "./lod.ts";
import { type Batch, type Domain, TileFailed, fetchBatch } from "./tiles.ts";

/** Tiles fetched at once. */
const PARALLEL = 6;

export interface CatalogueEvents {
  /** A tile's points, to draw. */
  onBatch: (batch: Batch) => void;
  /** The counts or credits changed. */
  onChange: () => void;
}

/** The tiles held for `release`, fetched as `update` is told where the eye is. */
export class Catalogue {
  readonly release: number;
  readonly domains: readonly Domain[];
  private readonly events: CatalogueEvents;
  private readonly options: ReadOptions;
  private readonly held = new Set<string>();
  private readonly pending = new Set<string>();
  private heldPoints = 0;
  private eye: Vec3 = [0, 0, 0];
  /** Points drawn of each kind of value, by its name. */
  readonly kinds = new Map<string, number>();
  /** The domains with a point drawn. */
  readonly drawnDomains = new Set<string>();
  failed = 0;

  constructor(release: number, domains: readonly Domain[], events: CatalogueEvents, options: ReadOptions = {}) {
    this.release = release;
    this.domains = domains;
    this.events = events;
    this.options = options;
  }

  /** Fetches the tiles the view from `eye` needs next. */
  update(eye: Vec3): void {
    this.eye = eye;
    const all = this.domains.flatMap((d) => d.tiles);
    const room = PARALLEL - this.pending.size;
    if (room <= 0) return;
    for (const tile of nextTiles(all, this.held, this.pending, this.heldPoints, eye, room)) {
      const domain = this.domains.find((d) => d.name === tile.domain);
      if (domain === undefined) continue;
      this.pending.add(tile.id);
      this.heldPoints += tile.points;
      void fetchBatch(domain, tile, this.release, this.options)
        .then((batch) => {
          this.count(domain, batch);
          this.events.onBatch(batch);
        })
        .catch((error: unknown) => {
          if (this.options.signal?.aborted === true) return;
          if (!(error instanceof TileFailed)) console.warn(error);
          this.failed += 1;
        })
        .finally(() => {
          this.pending.delete(tile.id);
          this.held.add(tile.id);
          this.events.onChange();
          if (this.options.signal?.aborted !== true) this.update(this.eye);
        });
    }
  }

  private count(domain: Domain, batch: Batch): void {
    const names = domain.manifest.encoding.kinds;
    for (const kind of batch.kinds) {
      const name = names[kind] ?? "unknown";
      this.kinds.set(name, (this.kinds.get(name) ?? 0) + 1);
    }
    if (batch.kinds.length > 0) this.drawnDomains.add(domain.name);
  }

  /** The points held. */
  get points(): number {
    let total = 0;
    for (const count of this.kinds.values()) total += count;
    return total;
  }

  /** The sources of every domain with a point drawn, as their manifests credit them. */
  credits(): { domain: string; id: string; title: string; citation: string; credit: string }[] {
    return this.domains
      .filter((d) => this.drawnDomains.has(d.name))
      .flatMap((d) =>
        d.manifest.sources.map((s) => ({ domain: d.name, id: s.id, title: s.title, citation: s.citation, credit: s.credit })),
      );
  }
}
