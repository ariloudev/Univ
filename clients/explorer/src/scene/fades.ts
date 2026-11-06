// How opaque each tile's points are drawn as the tile arrives and goes (draw-the-catalogues):
// rising from none to full over `FADE_MS` once its points arrive, and falling to none over
// `FADE_MS` before they are drawn no more, so that detail comes and goes without a jump.

/** How long a tile takes to fade in or out, milliseconds. */
export const FADE_MS = 300;

/** A tile's fade: rising or falling, as from `from`. */
interface Fade {
  from: number;
  leaving: boolean;
}

/** The fades of the tiles drawn, by id. */
export class Fades {
  private readonly fades = new Map<string, Fade>();

  /** The tile `id` arrives at `now`: it rises from as opaque as it is, so that a tile that goes
   * and comes back before it is gone does not blink. */
  arrive(id: string, now: number): void {
    const was = this.opacity(id, now);
    this.fades.set(id, { from: now - was * FADE_MS, leaving: false });
  }

  /** The tile `id` goes at `now`: it falls from as opaque as it is. */
  leave(id: string, now: number): void {
    if (!this.fades.has(id)) return;
    const was = this.opacity(id, now);
    this.fades.set(id, { from: now - (1 - was) * FADE_MS, leaving: true });
  }

  /** How opaque the tile `id` is at `now`, 0 to 1: 0 for a tile not drawn. */
  opacity(id: string, now: number): number {
    const fade = this.fades.get(id);
    if (fade === undefined) return 0;
    const t = Math.min(1, Math.max(0, (now - fade.from) / FADE_MS));
    return fade.leaving ? 1 - t : t;
  }

  /** The tiles whose fade out has ended by `now`, which are forgotten. */
  gone(now: number): string[] {
    const ended = [...this.fades].filter(([, f]) => f.leaving && now - f.from >= FADE_MS).map(([id]) => id);
    for (const id of ended) this.fades.delete(id);
    return ended;
  }

  /** Whether any tile is fading in or out at `now`. */
  moving(now: number): boolean {
    for (const fade of this.fades.values()) if (now - fade.from < FADE_MS) return true;
    return false;
  }

  /** Forgets every fade, as when the view closes. */
  clear(): void {
    this.fades.clear();
  }
}
