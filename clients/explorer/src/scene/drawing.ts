// What the view draws besides points, and what draws it.

import type { View } from "./camera.ts";
import type { Mat3, Vec3 } from "./vec.ts";

/** A colour, red, green, blue and opacity, each 0 to 1. */
export type Rgba = readonly [number, number, number, number];

/** Line segments to draw: pairs of offsets from `centre`, in one colour. */
export interface Lines {
  centre: Vec3;
  offsets: Float32Array;
  colour: Rgba;
  /** Where the lines are rings that fade as they crowd on the view, the distance rings, the
   * megaparsecs between neighbours. */
  fade?: number | undefined;
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
  draw: (view: View, toFrame: Mat3, width: number, height: number) => void;
  setLines: (name: string, lines: Lines | undefined) => void;
}
