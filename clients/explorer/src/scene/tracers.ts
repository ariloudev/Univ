// The Galaxy's tracers (draw-the-catalogues): open and globular clusters, Cepheids, RR Lyrae stars
// and masers, each source in a colour of its own, chosen away from the stars' black-body colours
// (red through white to pale blue), their grey, and the blue of the other domains' points, and
// drawn twice the size of a star, so that they stand out among the stars. A tracer matched to a
// Gaia star whose distance it takes is drawn in that source's colour, so the domain's manifest
// lists the Gaia sources too, and there is a colour for each of its seven sources.

import type { Rgba } from "./drawing.ts";
import { POINT_OPACITY } from "./kinds.ts";
import { toBytes } from "./starColour.ts";

/** The scale domain of the Galaxy's tracers. */
export const TRACERS_DOMAIN = "galaxy_tracers";

/** The diameter of a point, in device pixels. */
export const POINT_SIZE = 2;

/** The colours of the tracer sources, by their place in the domain's manifest, then again from
 * the first: green, magenta, violet, teal, lime, crimson and forest green. */
export const TRACER_COLOURS: readonly Rgba[] = [
  [0.3, 0.85, 0.35, POINT_OPACITY],
  [0.95, 0.3, 0.85, POINT_OPACITY],
  [0.65, 0.4, 1, POINT_OPACITY],
  [0.15, 0.85, 0.75, POINT_OPACITY],
  [0.8, 0.95, 0.2, POINT_OPACITY],
  [0.9, 0.1, 0.35, POINT_OPACITY],
  [0.1, 0.6, 0.15, POINT_OPACITY],
];

/** The colour of the tracer source at `index` in its manifest. */
export function tracerColour(index: number): Rgba {
  const colour = TRACER_COLOURS[index % TRACER_COLOURS.length];
  if (colour === undefined) throw new Error("no tracer colours");
  return colour;
}

const BYTES = TRACER_COLOURS.map(toBytes);

/** Each point's colour, as red, green, blue and opacity bytes, from its source's index. */
export function tracerBytes(sources: Uint16Array): Uint8Array {
  const bytes = new Uint8Array(sources.length * 4);
  sources.forEach((source, i) => {
    const colour = BYTES[source % BYTES.length];
    if (colour !== undefined) bytes.set(colour, i * 4);
  });
  return bytes;
}

/** The diameter a point of `domain` is drawn at, in device pixels: twice a star's for a tracer. */
export const pointSizeOf = (domain: string): number => (domain === TRACERS_DOMAIN ? 2 * POINT_SIZE : POINT_SIZE);
