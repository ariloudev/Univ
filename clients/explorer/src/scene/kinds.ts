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

/** Beyond this distance from the focus, megaparsecs, the points grow fainter as the eye
 * withdraws, so that whole surveys seen from afar add up to their shape rather than to white. */
const FADE_FROM = 1000;
/** The faintest the points grow. */
const FADE_FLOOR = 0.03;

/** The opacity the points are drawn at, of their kind's, with the eye `distance` from the focus:
 * in proportion to the inverse distance, as the points crowd into fewer pixels. */
export const pointOpacity = (distance: number): number => Math.max(FADE_FLOOR, Math.min(1, FADE_FROM / distance));

/** The colours of `KINDS`, in order, for the renderer. */
export const KIND_COLOURS: readonly Rgba[] = KINDS.map((kind) => COLOURS[kind]);

/** The colour of structures' outlines and markers, after the kinds' in the renderer's table. */
export const STRUCTURE_COLOUR: Rgba = [1, 0.55, 0.35, 0.9];
/** The colour of a structure drawn by its bounding box, marked approximate: fainter. */
export const APPROXIMATE_COLOUR: Rgba = [STRUCTURE_COLOUR[0], STRUCTURE_COLOUR[1], STRUCTURE_COLOUR[2], 0.3];
/** The colour of the chosen structure's outline, markers and name (see-the-structures): white,
 * apart from the structures' orange and from every kind of value's colour. */
export const CHOSEN_COLOUR: Rgba = [1, 1, 1, 1];
/** The index of `STRUCTURE_COLOUR` in the renderer's table of colours. */
export const MARKER_INDEX = KINDS.length;
/** The diameter of a structure's markers, device pixels. */
const MARKER_SIZE = 6;
/** The diameter of the chosen structure's markers: a structure placed by a point shows as chosen. */
const CHOSEN_MARKER_SIZE = 16;

/** How a structure drawn in `colour` is drawn: so, or in the chosen colour, its markers larger. */
export const structureStyle = (colour: Rgba, chosen: boolean): { colour: Rgba; markerSize: number } =>
  chosen ? { colour: CHOSEN_COLOUR, markerSize: CHOSEN_MARKER_SIZE } : { colour, markerSize: MARKER_SIZE };

const css = ([r, g, b]: Rgba): string => `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;

/** The colour of `kind` for CSS. */
export const cssColour = (kind: KindOfValue): string => css(COLOURS[kind]);

/** The colour of structures for CSS, as their names are written. */
export const STRUCTURE_CSS = css(STRUCTURE_COLOUR);

/** The chosen structure's colour for CSS, as its name is written. */
export const CHOSEN_CSS = css(CHOSEN_COLOUR);
