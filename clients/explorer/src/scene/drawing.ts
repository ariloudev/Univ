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
}

/** What the view's controller draws on. */
export interface Drawer {
  draw: (view: View, toFrame: Mat3, width: number, height: number) => void;
  setLines: (name: string, lines: Lines | undefined) => void;
}
