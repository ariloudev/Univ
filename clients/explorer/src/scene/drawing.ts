// What the view draws besides points, and what draws it.

import type { View } from "./camera.ts";
import type { Mat3, Vec3 } from "./vec.ts";

/** A colour, red, green, blue and opacity, each 0 to 1. */
export type Rgba = readonly [number, number, number, number];

/** How far apart on the view, CSS pixels, neighbouring lines of a family that fade as they crowd
 * lie where a line is gone and where it is whole: between them it fades. */
export const FADE = { gone: 2, full: 6 } as const;

/** Line segments to draw: pairs of offsets from `centre`, in one colour. */
export interface Lines {
  centre: Vec3;
  offsets: Float32Array;
  colour: Rgba;
  /** For lines of a family that fade as they crowd on the view, such as the distance rings: for
   * each end in `offsets`, the ICRS vector, megaparsecs, from it to the same place on the next
   * line of the family, zero where a line does not fade. */
  neighbours?: Float32Array | undefined;
  /** A symbol drawn at a fixed size on the view about `centre`, such as an attractor's. */
  symbol?: FixedSymbol | undefined;
}

/** Segments drawn at a fixed size on the view: `offsets` in units of `pixels` device pixels,
 * whatever the eye's distance, in lines no wider than `maxWidth` device pixels. */
export interface FixedSymbol {
  offsets: Float32Array;
  pixels: number;
  maxWidth: number;
}

/** What the view's controller draws on. */
export interface Drawer {
  /** Draws `view` on a canvas `width` × `height` device pixels, `ratio` of them to a CSS pixel. */
  draw: (view: View, toFrame: Mat3, width: number, height: number, ratio: number) => void;
  setLines: (name: string, lines: Lines | undefined) => void;
}
