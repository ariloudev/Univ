// The colour of each kind of value (draw-the-catalogues): measured, estimated, inferred and
// reconstructed distances, as the tile encoding lists the kinds. Points add up where they
// crowd, so each is half opaque: a survey's dense core stays readable rather than white.

const KIND_COLOURS: Record<string, readonly [number, number, number, number]> = {
  measured: [0.42, 0.72, 1, 0.5],
  estimated: [1, 0.81, 0.42, 0.5],
  inferred: [0.84, 0.55, 1, 0.5],
  reconstructed: [0.42, 1, 0.69, 0.5],
};

/** The colour of `kind` for CSS. */
export function cssColour(kind: string): string {
  const [r, g, b] = KIND_COLOURS[kind] ?? [1, 1, 1, 1];
  return `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;
}

/** The colours of `kinds`, in their order, for the renderer. */
export function coloursOf(kinds: readonly string[]): (readonly [number, number, number, number])[] {
  return kinds.map((kind) => KIND_COLOURS[kind] ?? [1, 1, 1, 1]);
}
