// The grid across the view (frame-the-view-as-an-atlas): the lines of sight along the round
// longitudes and latitudes the edges' ticks mark, at the ticks' steps, in the chosen axes. Each
// line is a direction, the same from every point of space, so it turns with the view and the axes
// and stays still while the eye moves or zooms. A line of longitude is a great circle and falls
// straight on the view; a line of latitude bends.

import type { View } from "./camera.ts";
import { eyeAxes, focalPixels } from "./camera.ts";
import { type EdgeTicks, across, edgePoint, sight } from "./graticule.ts";
import { type Mat3, type Vec3, apply, multiply, transpose } from "./vec.ts";

/** Degrees between the samples taken along each line. */
const SAMPLE = 1;

/** Degrees past the view's corners within which samples are kept, more than one sample apart,
 * so that every line runs on past the view's edge. */
const MARGIN = 2;

/** The latitude, north and south, at which a line of longitude stops, unless it lies at a
 * multiple of `THROUGH` or runs on to a tick past it; degrees. */
const STOP = 80;

/** The lines of longitude that run on to the poles lie at multiples of this, degrees: 6 hours of
 * right ascension. */
const THROUGH = 90;

export interface GridLine {
  coordinate: "longitude" | "latitude";
  /** The longitude or latitude along which it runs, degrees. */
  value: number;
  /** Whether it is the axes' equator. */
  equator: boolean;
  /** Its parts in the view, each a run of CSS pixels. */
  runs: [number, number][][];
}

const RAD = Math.PI / 180;

/** The direction of longitude `lon` and latitude `lat`, degrees, in the axes it is given in. */
const direction = (lon: number, lat: number): Vec3 => {
  const c = Math.cos(lat * RAD);
  return [c * Math.cos(lon * RAD), c * Math.sin(lon * RAD), Math.sin(lat * RAD)];
};

/** `value` without the error of the arithmetic that stepped to it. */
const tidy = (value: number): number => Math.round(value * 1e6) / 1e6;

/** The multiples of `step` above `from` and below `to`, `from` itself too where `including`. */
function multiples(step: number, from: number, to: number, including: boolean): number[] {
  const values: number[] = [];
  for (let k = Math.ceil(from / step); k * step < to; k += 1) {
    const value = tidy(k * step);
    if (value < to && (value > from || (including && value === from))) values.push(value);
  }
  return values;
}

/** The values from `from` to `to` every `SAMPLE` degrees, both ends included. */
function samples(from: number, to: number): number[] {
  const count = Math.max(1, Math.ceil((to - from) / SAMPLE));
  return Array.from({ length: count + 1 }, (_, i) => from + ((to - from) * i) / count);
}

/** How far north and south, degrees, the line of longitude `lon` runs: to `STOP`, to a pole
 * for one at a multiple of `THROUGH`, or a sample past the farthest tick of it on an edge. */
function reachOf(lon: number, ticked: readonly { lon: number; lat: number }[]): { south: number; north: number } {
  if (lon % THROUGH === 0) return { south: 90, north: 90 };
  let south = STOP;
  let north = STOP;
  for (const t of ticked) {
    if (t.lon !== lon) continue;
    if (t.lat > 0) north = Math.max(north, Math.min(90, t.lat + SAMPLE));
    else south = Math.max(south, Math.min(90, -t.lat + SAMPLE));
  }
  return { south, north };
}

/** The grid's lines in a viewport `width` × `height` CSS pixels, for `view` in the axes `toFrame`
 * turns the ICRS into, at the steps of the edges' ticks `edges`. */
export function gridLines(view: View, toFrame: Mat3, width: number, height: number, edges: readonly EdgeTicks[]): GridLine[] {
  const f = focalPixels(height);
  // From the chosen axes to the eye's: right, up and backward.
  const toEye = multiply(eyeAxes(view, toFrame), transpose(toFrame));
  // A direction is kept within this cosine of the line of sight: past the view's corners.
  const corner = Math.atan(Math.hypot(width / 2, height / 2) / f) / RAD;
  const least = Math.cos(Math.min(89, corner + MARGIN) * RAD);
  const trace = (points: readonly (readonly [number, number])[]): [number, number][][] => {
    const runs: [number, number][][] = [];
    let run: [number, number][] = [];
    for (const [lon, lat] of points) {
      const [x, y, z] = apply(toEye, direction(lon, lat));
      const depth = -z;
      if (depth >= least) run.push([width / 2 + (x * f) / depth, height / 2 - (y * f) / depth]);
      else if (run.length > 0) {
        runs.push(run);
        run = [];
      }
    }
    if (run.length > 0) runs.push(run);
    return runs.filter((r) => r.length > 1);
  };
  const stepOf = (longitude: boolean) => edges.find((e) => across(e.edge) === longitude && e.step !== undefined)?.step;
  const sightAt = sight(view, toFrame, width, height);
  // Where each tick of longitude lies on the sky: a line of it runs on to its tick.
  const ticked = edges
    .filter((e) => across(e.edge))
    .flatMap((e) => e.ticks.map((t) => ({ lon: t.value, lat: sightAt(...edgePoint(e.edge, t.at, 0, width, height)).lat })));
  const lines: GridLine[] = [];
  const lonStep = stepOf(true);
  if (lonStep !== undefined) {
    for (const lon of multiples(lonStep, 0, 360, true)) {
      const { south, north } = reachOf(lon, ticked);
      const runs = trace(samples(-south, north).map((lat) => [lon, lat] as const));
      if (runs.length > 0) lines.push({ coordinate: "longitude", value: lon, equator: false, runs });
    }
  }
  const latStep = stepOf(false);
  if (latStep !== undefined) {
    for (const lat of multiples(latStep, -90, 90, false)) {
      const runs = trace(samples(0, 360).map((lon) => [lon, lat] as const));
      if (runs.length > 0) lines.push({ coordinate: "latitude", value: lat, equator: lat === 0, runs });
    }
  }
  return lines;
}
