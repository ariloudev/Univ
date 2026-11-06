// A tile's points in a shuffled order (draw-the-catalogues): the first of them, however many,
// are an even sample of all, so that a share of a tile can be drawn by drawing its first points.
// The order follows from the tile's id alone, so that every read of a tile gives the same one.

/** A 32-bit FNV-1a hash of `text`. */
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Mulberry32: a small generator of numbers in [0, 1) from a 32-bit seed. */
function generator(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** `items` shuffled in place by a Fisher–Yates shuffle seeded by `key`, and returned. */
export function shuffled<T>(items: T[], key: string): T[] {
  const next = generator(hash(key));
  for (let i = items.length - 1; i > 0; i -= 1) {
    const j = Math.floor(next() * (i + 1));
    const item = items[i] as T;
    items[i] = items[j] as T;
    items[j] = item;
  }
  return items;
}
