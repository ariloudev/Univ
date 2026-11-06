// The kinds of value and the one colour of the points outside the stars domain
// (draw-the-catalogues). Every batch of points indexes the one order of kinds below, whatever order
// its manifest lists them in, so the key counts them by kind. Points add up where they crowd, so
// each is half opaque: a survey's dense core stays readable rather than white.

import type { KindOfValue } from "univ-tile-decoder";

import type { Detail } from "../catalogue/lod.ts";
import type { Rgba } from "./drawing.ts";

/** The kinds of value, in the order points index them and the renderer colours them. */
export const KINDS: readonly KindOfValue[] = ["measured", "estimated", "inferred", "reconstructed"];

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

/** A colour, red, green and blue each 0 to 1, for CSS. */
export const css = ([r = 0, g = 0, b = 0]: readonly number[]): string =>
  `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;

/** The colour of the points outside the stars domain for CSS. */
export const POINT_CSS = css(POINT_COLOUR);

/** The colour of structures for CSS, as their names are written. */
export const STRUCTURE_CSS = css(STRUCTURE_COLOUR);

/** The chosen structure's colour for CSS, as its name is written. */
export const CHOSEN_CSS = css(CHOSEN_COLOUR);
