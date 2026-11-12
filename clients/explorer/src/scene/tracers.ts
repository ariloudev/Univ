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

/** The tracers by object index, in the order of the key and of their switches: first those of no
 * kind, or of a kind this table does not hold (green), then each kind of object (teal, magenta,
 * lime, violet, forest green and crimson), with what the key and the switches call them. The
 * index is one more than the kind's place among the kinds, as a version 4 tile codes it, and 0,
 * which no switch turns off, is a tracer of no kind. */
export const TRACER_OBJECTS = [
  { id: "unknown", title: "Tracers of unknown kind", colour: [0.3, 0.85, 0.35, POINT_OPACITY] },
  { id: "luminous_hot_star", title: "Luminous hot stars", colour: [0.15, 0.85, 0.75, POINT_OPACITY] },
  { id: "open_cluster", title: "Open clusters", colour: [0.95, 0.3, 0.85, POINT_OPACITY] },
  { id: "star_forming_region", title: "Star-forming regions", colour: [0.8, 0.95, 0.2, POINT_OPACITY] },
  { id: "classical_cepheid", title: "Classical Cepheids", colour: [0.65, 0.4, 1, POINT_OPACITY] },
  { id: "rr_lyrae_star", title: "RR Lyrae stars", colour: [0.1, 0.6, 0.15, POINT_OPACITY] },
  { id: "globular_cluster", title: "Globular clusters", colour: [0.9, 0.1, 0.35, POINT_OPACITY] },
] as const satisfies readonly { id: ObjectKind | "unknown"; title: string; colour: Rgba }[];

/** The kinds of object a Galaxy tracer is, in the order of `TRACER_OBJECTS`. */
export type TracerKind = Exclude<(typeof TRACER_OBJECTS)[number]["id"], "unknown">;
export const TRACER_KINDS: readonly TracerKind[] = TRACER_OBJECTS.slice(1).map((o) => o.id as TracerKind);

/** Whether `name` is a kind of object of the Galaxy's tracers. */
export const isTracerKind = (name: string): name is TracerKind => (TRACER_KINDS as readonly string[]).includes(name);

/** The object index of a tracer of `kind`: its place in `TRACER_OBJECTS`, 0 for none. */
export const objectIndex = (kind: string | undefined): number => (kind === undefined ? 0 : (TRACER_KINDS as readonly string[]).indexOf(kind) + 1);

/** The colours of the tracers by object index. */
export const TRACER_COLOURS: readonly Rgba[] = TRACER_OBJECTS.map((o) => o.colour);

/** The colour of a tracer of object index `object`: that of unknown kind for an index of none. */
export const tracerColour = (object: number): Rgba => (TRACER_OBJECTS[object] ?? TRACER_OBJECTS[0]).colour;

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

/** Whether `domains` hold the Galaxy's tracers by kind of object: their tiles, from encoding
 * version 4, give each point's kind. */
export const holdsTracerKinds = (domains: Iterable<{ name: string; manifest: { encoding: { object_kinds?: readonly string[] | undefined } } }>): boolean =>
  [...domains].some((d) => d.name === TRACERS_DOMAIN && d.manifest.encoding.object_kinds !== undefined);

/** The diameter a point of `domain` is drawn at, in device pixels: twice a star's for a tracer. */
export const pointSizeOf = (domain: string): number => (domain === TRACERS_DOMAIN ? 2 * POINT_SIZE : POINT_SIZE);
