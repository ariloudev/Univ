// The kinds of value and the one colour of the points outside the stars domain
// (draw-the-catalogues). Every batch of points indexes the one order of kinds below, whatever order
// its manifest lists them in, so the key counts them by kind. Points add up where they crowd, so
// each is half opaque: a survey's dense core stays readable rather than white.

import type { KindOfValue } from "univ-tile-decoder";

import type { Detail } from "../catalogue/lod.ts";
import type { Rgba } from "./drawing.ts";

/** The kinds of value, in the order points index them and the renderer colours them. */
export const KINDS: readonly KindOfValue[] = ["measured", "estimated", "inferred", "reconstructed"];

/** The mark of each kind of value, in the key and beside each value of an object's record: full
 * for measured, half for estimated, an open ring for inferred and a dotted one for
 * reconstructed. */
export const KIND_MARKS: Record<KindOfValue, string> = { measured: "●", estimated: "◐", inferred: "○", reconstructed: "◌" };

/** Each kind's index in `KINDS`. */
export const KIND_INDEX: Record<KindOfValue, number> = { measured: 0, estimated: 1, inferred: 2, reconstructed: 3 };

/** How opaque every point is drawn, of a kind or a star: half. */
export const POINT_OPACITY = 0.5;

/** The opacity, of their own, the points inside a structure the view is culled to are drawn at
 * (see-the-structures): fully opaque, whatever the eye's distance, as those outside no longer
 * crowd them. */
export const CULLED_OPACITY = 1 / POINT_OPACITY;

/** The colour of every point outside the stars domain, whatever the kind of value of its
 * distance. */
const POINT_COLOUR: Rgba = [0.42, 0.72, 1, POINT_OPACITY];

/** Beyond this distance from the focus, megaparsecs, the points grow fainter as the eye
 * withdraws, so that whole surveys seen from afar add up to their shape rather than to white. */
const FADE_FROM = 1000;
/** The faintest the points grow. */
const FADE_FLOOR = 0.03;

/** The opacity the points are drawn at, of their kind's, with the eye `distance` from the focus:
 * in proportion to the inverse distance, as every point crowds into fewer pixels. */
export const pointOpacity = (distance: number): number => Math.max(FADE_FLOOR, Math.min(1, FADE_FROM / distance));

/** The opacity a domain's points are drawn at with its level of detail `detail`, the eye
 * `distance` from the focus: all of it while the view still thins the domain as the eye withdraws,
 * within the distance where it starts to need the level below the root, and fading beyond it as
 * the points crowd again; never fainter than `pointOpacity`, which is all a domain of one level,
 * or one with level of detail off, gets. */
export function detailOpacity(detail: Detail, distance: number): number {
  const thinned = Math.min(1, (detail.onsets[1] ?? 0) / distance);
  return Math.max(pointOpacity(distance), thinned);
}

/** The colours of `KINDS`, in order, for the renderer's table: the one colour for each. */
export const KIND_COLOURS: readonly Rgba[] = KINDS.map(() => POINT_COLOUR);

/** The colour of structures' names, and of the outlines and markers of a kind of structure with
 * no colour of its own. */
export const STRUCTURE_COLOUR: Rgba = [1, 0.55, 0.35, 0.9];

/** The families the Show drawer groups the kinds of structure in (choose-what-is-drawn), in
 * its order; a kind of none of them is shown under "other". */
const FAMILIES = ["denser", "emptier", "walls and threads", "distant arcs and groups", "flows", "the Galaxy's arms"] as const;

/** Each kind of structure's colour for its outline and markers (see-the-structures), apart from
 * one another, from the points' blue and from the chosen structure's white, and the family the
 * Show drawer lists it in: violets for what is emptier than its surroundings and for the
 * repellers, oranges and reds for what is denser, greens for walls and filaments, yellows for
 * the quasar and burst structures of the distant universe and for a basin of attraction,
 * magentas for the attractors and the shells, and a pale gold for the Galaxy's model arms, apart
 * from the tracers' colours they lie over. A `faint` kind, a model arm's extrapolation, is drawn
 * fainter than its arm and beneath every other outline, so that the fitted range stands out
 * along it. */
const STRUCTURE_KINDS: Readonly<Record<string, { rgb: readonly [number, number, number]; family: (typeof FAMILIES)[number]; faint?: true }>> = {
  void: { rgb: [0.62, 0.45, 1], family: "emptier" },
  repeller: { rgb: [0.8, 0.35, 1], family: "flows" },
  attractor: { rgb: [1, 0.25, 0.95], family: "flows" },
  supercluster: { rgb: [1, 0.55, 0.35], family: "denser" },
  supercluster_complex: { rgb: [1, 0.33, 0.3], family: "denser" },
  basin: { rgb: [1, 0.78, 0.3], family: "flows" },
  overdensity: { rgb: [0.95, 0.62, 0.55], family: "denser" },
  wall: { rgb: [0.45, 0.95, 0.35], family: "walls and threads" },
  filament: { rgb: [0.2, 0.8, 0.65], family: "walls and threads" },
  quasar_group: { rgb: [1, 0.92, 0.2], family: "distant arcs and groups" },
  ring: { rgb: [0.85, 1, 0.5], family: "distant arcs and groups" },
  arc: { rgb: [0.7, 0.85, 0.15], family: "distant arcs and groups" },
  shell: { rgb: [1, 0.5, 0.78], family: "denser" },
  spiral_arm: { rgb: [1, 0.88, 0.55], family: "the Galaxy's arms" },
  spiral_arm_extrapolation: { rgb: [1, 0.88, 0.55], family: "the Galaxy's arms", faint: true },
};

/** Whether `kind` of structure is drawn faint, beneath every other outline. */
export const drawnBeneath = (kind: string): boolean => STRUCTURE_KINDS[kind]?.faint === true;

/** The families of the kinds of structure, each with its kinds, in the Show drawer's order. */
export const STRUCTURE_FAMILIES: readonly { family: string; kinds: readonly string[] }[] = FAMILIES.map((family) => ({
  family,
  kinds: Object.keys(STRUCTURE_KINDS).filter((kind) => STRUCTURE_KINDS[kind]?.family === family),
}));

/** The way the reconstructed flow runs at each kind of structure that is a centre of a flow
 * (see-the-structures): in to an attractor, where a peculiar-velocity field converges, and out
 * from a repeller, where it diverges. Both are the fixed points of one field, a sink and a
 * source, and are drawn alike, as a sphere with arrows along the flow. */
export type Flow = "in" | "out";
export const FLOWS: Readonly<Record<string, Flow>> = { attractor: "in", repeller: "out" };

/** The radius of the sphere an attractor or a repeller is drawn as, device pixels whatever the
 * eye's distance: a symbol, not its extent. Its arrows reach `FLOW_REACH` radii. */
export const FLOW_PIXELS = 24;

/** The widest line a flow symbol is drawn in, device pixels: the chosen structure's lines would
 * fill a sphere so small. */
export const FLOW_LINE_WIDTH = 2.5;

/** How opaque a structure's outline and markers are drawn, and fainter where it is drawn by its
 * bounding box, marked approximate, or is of a faint kind. */
const STRUCTURE_OPACITY = 0.9;
const APPROXIMATE_OPACITY = 0.3;

/** The colour `kind` of structure is drawn in, fainter where it is drawn `approximate` or the
 * kind is faint. */
export function structureColour(kind: string, approximate: boolean): Rgba {
  const [r, g, b] = STRUCTURE_KINDS[kind]?.rgb ?? STRUCTURE_COLOUR;
  return [r, g, b, approximate || drawnBeneath(kind) ? APPROXIMATE_OPACITY : STRUCTURE_OPACITY];
}

/** `kind` of structure's colour for CSS, as the panel of what is drawn shows it. */
export const structureCss = (kind: string): string => css(structureColour(kind, false));
/** The colour of the chosen structure's outline, markers and name (see-the-structures): white,
 * apart from the structures' orange and from every kind of value's colour. */
export const CHOSEN_COLOUR: Rgba = [1, 1, 1, 1];
/** The index of `STRUCTURE_COLOUR` in the renderer's table of colours. */
export const MARKER_INDEX = KINDS.length;
/** The diameter of a structure's markers, device pixels. */
const MARKER_SIZE = 6;
/** The diameter of the chosen structure's markers: a structure placed by a point shows as chosen. */
const CHOSEN_MARKER_SIZE = 16;
/** The width of a structure's outline, device pixels. */
const LINE_WIDTH = 1.5;
/** The width of the chosen structure's outline: thick enough that it shows as chosen among
 * the others it crosses. */
const CHOSEN_LINE_WIDTH = 5;

/** How a structure drawn in `colour` is drawn: so, or in the chosen colour, its markers larger
 * and its outline thicker. */
export const structureStyle = (colour: Rgba, chosen: boolean): { colour: Rgba; markerSize: number; lineWidth: number } =>
  chosen
    ? { colour: CHOSEN_COLOUR, markerSize: CHOSEN_MARKER_SIZE, lineWidth: CHOSEN_LINE_WIDTH }
    : { colour, markerSize: MARKER_SIZE, lineWidth: LINE_WIDTH };

/** A colour, red, green and blue each 0 to 1, for CSS. */
export const css = ([r = 0, g = 0, b = 0]: readonly number[]): string =>
  `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;

/** The colour of the points outside the stars domain for CSS. */
export const POINT_CSS = css(POINT_COLOUR);

/** The colour of structures for CSS, as their names are written. */
export const STRUCTURE_CSS = css(STRUCTURE_COLOUR);

/** The chosen structure's colour for CSS, as its name is written. */
export const CHOSEN_CSS = css(CHOSEN_COLOUR);
