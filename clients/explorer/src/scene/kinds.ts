// The colour of each kind of value (draw-the-catalogues). Every batch of points indexes the one
// order of kinds below, whatever order its manifest lists them in. Points add up where they
// crowd, so each is half opaque: a survey's dense core stays readable rather than white.

import type { KindOfValue } from "univ-tile-decoder";

import type { Rgba } from "./drawing.ts";

/** The kinds of value, in the order points index them and the renderer colours them. */
export const KINDS: readonly KindOfValue[] = ["measured", "estimated", "inferred", "reconstructed"];

/** Each kind's index in `KINDS`. */
export const KIND_INDEX: Record<KindOfValue, number> = { measured: 0, estimated: 1, inferred: 2, reconstructed: 3 };

const COLOURS: Record<KindOfValue, Rgba> = {
  measured: [0.42, 0.72, 1, 0.5],
  estimated: [1, 0.81, 0.42, 0.5],
  inferred: [0.84, 0.55, 1, 0.5],
  reconstructed: [0.42, 1, 0.69, 0.5],
};

/** The colours of `KINDS`, in order, for the renderer. */
export const KIND_COLOURS: readonly Rgba[] = KINDS.map((kind) => COLOURS[kind]);

/** The colour of `kind` for CSS. */
export function cssColour(kind: KindOfValue): string {
  const [r, g, b] = COLOURS[kind];
  return `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;
}
