// The Galaxy's tracers (draw-the-catalogues): luminous hot stars, open and globular clusters,
// star-forming regions, Cepheids and RR Lyrae stars, each kind of object in a colour of its own
// whichever source placed it, chosen away from the stars' black-body colours (red through white
// to pale blue), their grey, and the blue of the other domains' points, and drawn twice the size
// of a star, so that they stand out among the stars. A tracer whose tile gives no kind of object,
// as tiles before encoding version 4 give none, is drawn in a colour of its own too.

import type { ObjectKind } from "univ-tile-decoder";

import type { Rgba } from "./drawing.ts";
import { POINT_OPACITY } from "./kinds.ts";
import { toBytes } from "./starColour.ts";

/** The scale domain of the Galaxy's tracers. */
export const TRACERS_DOMAIN = "galaxy_tracers";

/** The diameter of a point, in device pixels. */
export const POINT_SIZE = 2;

/** The kinds of object a Galaxy tracer is, in the order of the key and of their switches. A
 * point's object index is one more than its kind's place here: 0 for a tracer of no kind, or of
 * a kind this list does not hold. */
export const TRACER_KINDS = [
  "luminous_hot_star",
  "open_cluster",
  "star_forming_region",
  "classical_cepheid",
  "rr_lyrae_star",
  "globular_cluster",
] as const satisfies readonly ObjectKind[];

export type TracerKind = (typeof TRACER_KINDS)[number];

/** Whether `name` is a kind of object of the Galaxy's tracers. */
export const isTracerKind = (name: string): name is TracerKind => (TRACER_KINDS as readonly string[]).includes(name);

/** The object index of a tracer of `kind`: one more than its place in `TRACER_KINDS`, 0 for none. */
export const objectIndex = (kind: string | undefined): number => (kind === undefined ? 0 : (TRACER_KINDS as readonly string[]).indexOf(kind) + 1);

/** The colours of the tracers by object index: first a tracer of unknown kind (green), then each
 * of `TRACER_KINDS` (teal, magenta, lime, violet, forest green and crimson). */
export const TRACER_COLOURS: readonly Rgba[] = [
  [0.3, 0.85, 0.35, POINT_OPACITY],
  [0.15, 0.85, 0.75, POINT_OPACITY],
  [0.95, 0.3, 0.85, POINT_OPACITY],
  [0.8, 0.95, 0.2, POINT_OPACITY],
  [0.65, 0.4, 1, POINT_OPACITY],
  [0.1, 0.6, 0.15, POINT_OPACITY],
  [0.9, 0.1, 0.35, POINT_OPACITY],
];

/** What the key and the switches call each kind of object's tracers, by object index. */
export const TRACER_TITLES: readonly string[] = [
  "Tracers of unknown kind",
  "Luminous hot stars",
  "Open clusters",
  "Star-forming regions",
  "Classical Cepheids",
  "RR Lyrae stars",
  "Globular clusters",
];

/** The colour of a tracer of object index `object`: that of unknown kind for an index of none. */
export function tracerColour(object: number): Rgba {
  const colour = TRACER_COLOURS[object] ?? TRACER_COLOURS[0];
  if (colour === undefined) throw new Error("no tracer colours");
  return colour;
}

const BYTES = TRACER_COLOURS.map(toBytes);

/** Each point's colour, as red, green, blue and opacity bytes, from its object index. */
export function tracerBytes(objects: Uint8Array): Uint8Array {
  const bytes = new Uint8Array(objects.length * 4);
  objects.forEach((object, i) => {
    const colour = BYTES[object] ?? BYTES[0];
    if (colour !== undefined) bytes.set(colour, i * 4);
  });
  return bytes;
}

/** The diameter a point of `domain` is drawn at, in device pixels: twice a star's for a tracer. */
export const pointSizeOf = (domain: string): number => (domain === TRACERS_DOMAIN ? 2 * POINT_SIZE : POINT_SIZE);
