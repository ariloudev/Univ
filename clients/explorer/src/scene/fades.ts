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
  /** The tiles fading out. */
  private readonly leaving = new Set<string>();
  /** When a fade last began: none runs past `FADE_MS` after it. */
  private last = Number.NEGATIVE_INFINITY;
  /** Whether a frame has been drawn since every fade ended, its tiles at their last opacity and
   * the tiles faded out let go. */
  private settled = true;

  /** The tile `id` arrives at `now`: it rises from as opaque as it is, so that a tile that goes
   * and comes back before it is gone does not blink. */
  arrive(id: string, now: number): void {
    const was = this.opacity(id, now);
    this.fades.set(id, { from: now - was * FADE_MS, leaving: false });
    this.leaving.delete(id);
    this.last = now;
    this.settled = false;
  }

  /** The tile `id` goes at `now`: it falls from as opaque as it is. */
  leave(id: string, now: number): void {
    if (!this.fades.has(id)) return;
    const was = this.opacity(id, now);
    this.fades.set(id, { from: now - (1 - was) * FADE_MS, leaving: true });
    this.leaving.add(id);
    this.last = now;
    this.settled = false;
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
    const ended: string[] = [];
    for (const id of this.leaving) {
      const from = this.fades.get(id)?.from ?? now - FADE_MS;
      if (now - from >= FADE_MS) ended.push(id);
    }
    for (const id of ended) {
      this.fades.delete(id);
      this.leaving.delete(id);
    }
    return ended;
  }

  /** A frame was drawn at `now`, after letting go of the tiles `gone` gave: once every fade has
   * ended by then, the frame shows each tile as it stays. */
  drawn(now: number): void {
    if (now - this.last >= FADE_MS && this.leaving.size === 0) this.settled = true;
  }

  /** Whether a frame is still to be drawn for the fades: one is under way, or none has been drawn
   * since the last ended. */
  moving(): boolean {
    return !this.settled;
  }

  /** Forgets every fade, as when the view closes. */
  clear(): void {
    this.fades.clear();
    this.leaving.clear();
  }
}
