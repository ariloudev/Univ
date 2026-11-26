// The Galaxy's tracers (draw-the-catalogues): luminous hot stars, open and globular clusters,
// star-forming regions, Cepheids and RR Lyrae stars, each kind of object in a colour of its own
// whichever source placed it, chosen away from the stars' black-body colours (red through white
// to pale blue), their grey, and the blue of the other domains' points, and drawn twice the size
// of a star, so that they stand out among the stars. A tracer whose tile gives no kind of object,
// as tiles before encoding version 4 give none, is drawn in a colour of its own too.

import type { ObjectKind } from "univ-tile-decoder";

import type { Rgba } from "./drawing.ts";
import { POINT_OPACITY } from "./kinds.ts";
import { NO_COLOUR_GREY, starColour, toBytes } from "./starColour.ts";

/** The scale domain of the Galaxy's tracers. */
export const TRACERS_DOMAIN = "galaxy_tracers";

/** The diameter of a point, in device pixels. */
export const POINT_SIZE = 2;

/** The opacity of a luminous hot star, the most numerous kind of tracer by far: a tenth of the
 * other kinds', so that the hundreds of thousands about the Sun, drawn in a dark slate teal, do
 * not add up to white. */
const HOT_STAR_OPACITY = 0.1 * POINT_OPACITY;

/** The tracers by object index, in the order of the key and of their switches: first those of no
 * kind, or of a kind this table does not hold (green), then each kind of object (slate teal, magenta,
 * lime, violet, forest green, crimson and umber), with what the key and the switches call them. The
 * index is one more than the kind's place among the kinds, as a version 4 tile codes it, and 0,
 * which no switch turns off, is a tracer of no kind. */
export const TRACER_OBJECTS = [
  { id: "unknown", title: "Tracers of unknown kind", colour: [0.3, 0.85, 0.35, POINT_OPACITY] },
  { id: "luminous_hot_star", title: "Luminous hot stars", colour: [0.1, 0.4, 0.5, HOT_STAR_OPACITY] },
  { id: "open_cluster", title: "Open clusters", colour: [0.95, 0.3, 0.85, POINT_OPACITY] },
  { id: "star_forming_region", title: "Star-forming regions", colour: [0.8, 0.95, 0.2, POINT_OPACITY] },
  { id: "classical_cepheid", title: "Classical Cepheids", colour: [0.65, 0.4, 1, POINT_OPACITY] },
  { id: "rr_lyrae_star", title: "RR Lyrae stars", colour: [0.1, 0.6, 0.15, POINT_OPACITY] },
  { id: "globular_cluster", title: "Globular clusters", colour: [0.9, 0.1, 0.35, POINT_OPACITY] },
  { id: "molecular_cloud", title: "Molecular clouds", colour: [0.55, 0.35, 0, POINT_OPACITY] },
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

/** The colour a star of colour index B−V `index` is drawn in. */
const ofIndex = (index: number): Rgba => starColour({ index, bands: "B-V" });

/** Each kind of object's natural colour, by object index as `TRACER_OBJECTS` (draw-the-catalogues):
 * the colour a star of the kind's typical colour index B−V is drawn in, a choice for each, being
 * the middle of what such stars show: −0.30 for a luminous hot star, an O9 to B0 dwarf along
 * Pecaut & Mamajek 2013's sequence; −0.10 for an open cluster, whose light its brightest B stars
 * give; 0.35 for an RR Lyrae star, between the type c's 0.2 and the type ab's 0.45 (Catelan &
 * Smith 2015); 0.70 for a classical Cepheid, between 0.4 and 1.0 over its cycle and its period,
 * and for a globular cluster, the middle of Harris 1996's integrated colours. A star-forming
 * region is the pink of hydrogen's Balmer emission, as the Galaxy model's knots; a molecular
 * cloud, which shines in no visible light, a dark dust brown; a tracer of unknown kind the grey
 * of a star without a colour index. */
const NATURAL_HUES: readonly Rgba[] = [
  NO_COLOUR_GREY,
  ofIndex(-0.3),
  ofIndex(-0.1),
  [1, 120 / 255, 170 / 255, POINT_OPACITY],
  ofIndex(0.7),
  ofIndex(0.35),
  ofIndex(0.7),
  [0.35, 0.25, 0.18, POINT_OPACITY],
];

/** The brightest channel of `colour`. */
const brightest = ([r, g, b]: Rgba): number => Math.max(r, g, b);

/** Each natural hue as bright as its kind's own colour, its brightest channel the same, so that
 * natural colours change a point's hue and neither its brightness nor its opacity. */
export const NATURAL_COLOURS: readonly Rgba[] = NATURAL_HUES.map((hue, object) => {
  const own = tracerColour(object);
  const scale = brightest(own) / brightest(hue);
  return [hue[0] * scale, hue[1] * scale, hue[2] * scale, own[3]];
});

/** The colour of a tracer of object index `object` in the key and the Show drawer: its natural
 * colour while `natural`, else its kind's. */
export const shownColour = (object: number, natural: boolean): Rgba =>
  natural ? (NATURAL_COLOURS[object] ?? NO_COLOUR_GREY) : tracerColour(object);

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

/** The cells along each edge of a tile in which the RR Lyrae stars are counted. */
const CROWD_CELLS = 64;
/** How many RR Lyrae stars of a tile share a cell before they grow fainter. */
const CROWD_LIMIT = 4;
/** The faintest an RR Lyrae star grows where they crowd: a tenth of its opacity. */
const CROWD_FLOOR = 0.1;

/** The cell of a tile of edge `edge` an offset `at` from its centre lies in along one axis. */
const cellAlong = (at: number, edge: number): number => Math.min(CROWD_CELLS - 1, Math.max(0, Math.floor((at / edge + 0.5) * CROWD_CELLS)));

/** Each point's colour, as `tracerBytes` gives it, with the RR Lyrae stars, which crowd in the
 * bulge but lie sparse in the halo and the Sagittarius stream, made fainter where more than
 * `CROWD_LIMIT` of the tile's share a cell, a 64th of its edge `edge`: by `CROWD_LIMIT` over
 * their number, never below a tenth. `offsets` are the points' places about the tile's centre,
 * in the unit of `edge`. */
export function tracerColoursOf(objects: Uint8Array, offsets: Float32Array, edge: number): Uint8Array {
  const bytes = tracerBytes(objects);
  const rrLyrae = objectIndex("rr_lyrae_star");
  const cells = new Int32Array(objects.length).fill(-1);
  const counts = new Map<number, number>();
  objects.forEach((object, i) => {
    if (object !== rrLyrae || !(edge > 0)) return;
    const [x, y, z] = [0, 1, 2].map((axis) => cellAlong(offsets[i * 3 + axis] ?? 0, edge));
    const cell = ((x ?? 0) * CROWD_CELLS + (y ?? 0)) * CROWD_CELLS + (z ?? 0);
    cells[i] = cell;
    counts.set(cell, (counts.get(cell) ?? 0) + 1);
  });
  cells.forEach((cell, i) => {
    const n = counts.get(cell) ?? 0;
    if (n <= CROWD_LIMIT) return;
    const at = i * 4 + 3;
    bytes[at] = Math.round((bytes[at] ?? 0) * Math.max(CROWD_FLOOR, CROWD_LIMIT / n));
  });
  return bytes;
}

/** Whether `domains` hold the Galaxy's tracers by kind of object: their tiles, from encoding
 * version 4, give each point's kind. */
export const holdsTracerKinds = (domains: Iterable<{ name: string; manifest: { encoding: { object_kinds?: readonly string[] | undefined } } }>): boolean =>
  [...domains].some((d) => d.name === TRACERS_DOMAIN && d.manifest.encoding.object_kinds !== undefined);

/** The diameter a point of `domain` is drawn at, in device pixels: twice a star's for a tracer. */
export const pointSizeOf = (domain: string): number => (domain === TRACERS_DOMAIN ? 2 * POINT_SIZE : POINT_SIZE);
