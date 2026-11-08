// The grid across the view (frame-the-view-as-an-atlas): the lines of sight along the round
// longitudes and latitudes the edges' ticks mark, at the ticks' steps, in the chosen axes. Each
// line is a direction, the same from every point of space, so it turns with the view and the axes
// and stays still while the eye moves or zooms. A line of longitude is a great circle and falls
// straight on the view; a line of latitude bends.

import type { View } from "./camera.ts";
import { eyeAxes, focalPixels, pixelOf } from "./camera.ts";
import { type Coordinate, type EdgeTicks, directionOf, edgePoint, lonLat, sight, tidy } from "./graticule.ts";
import { type Mat3, type Vec3, apply, multiply, scale, transpose } from "./vec.ts";

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

const RAD = Math.PI / 180;

export interface GridLine {
  coordinate: Coordinate;
  /** The longitude or latitude along which it runs, degrees. */
  value: number;
  /** Its parts in the view, each a run of CSS pixels. */
  runs: [number, number][][];
}

/** Whether `line` is the axes' equator, drawn heavier than the rest. */
export const isEquator = (line: GridLine): boolean => line.coordinate === "latitude" && line.value === 0;

/** The multiples of `step` above `from` and below `to`, `from` itself too where `including`. */
function multiples(step: number, from: number, to: number, including: boolean): number[] {
  const values: number[] = [];
  for (let k = Math.ceil(from / step); k * step < to; k += 1) {
    const value = tidy(k * step);
    if (value < to && (value > from || (including && value === from))) values.push(value);
  }
  return values;
}

/** The values from `from` to `to` every `SAMPLE` degrees at most, both ends included. */
function samples(from: number, to: number): number[] {
  const count = Math.max(1, Math.ceil((to - from) / SAMPLE));
  return Array.from({ length: count + 1 }, (_, i) => from + ((to - from) * i) / count);
}

/** How far north and south, degrees, each line of longitude with a tick on an edge runs: a
 * sample past its farthest tick, or `STOP` where that is nearer. */
function reaches(view: View, toFrame: Mat3, width: number, height: number, edges: readonly EdgeTicks[]): Map<number, { south: number; north: number }> {
  const sightAt = sight(view, toFrame, width, height);
  const reach = new Map<number, { south: number; north: number }>();
  for (const edge of edges) {
    if (edge.coordinate !== "longitude") continue;
    for (const tick of edge.ticks) {
      const { lat } = sightAt(...edgePoint(edge.edge, tick.at, 0, width, height));
      const found = reach.get(tick.value) ?? { south: STOP, north: STOP };
      const past = Math.min(90, Math.abs(lat) + SAMPLE);
      if (lat > 0) found.north = Math.max(found.north, past);
      else found.south = Math.max(found.south, past);
      reach.set(tick.value, found);
    }
  }
  return reach;
}

/** The grid's lines in a viewport `width` × `height` CSS pixels, for `view` in the axes `toFrame`
 * turns the ICRS into, at the steps of the edges' ticks `edges`. Only the lines that come within
 * the view are sampled, and of a line of latitude only its span there. */
export function gridLines(view: View, toFrame: Mat3, width: number, height: number, edges: readonly EdgeTicks[]): GridLine[] {
  const f = focalPixels(height);
  // From the chosen axes to the eye's: right, up and backward.
  const toEye = multiply(eyeAxes(view, toFrame), transpose(toFrame));
  // The line of sight, and the angle about it, degrees, within which directions are kept: past
  // the view's corners.
  const centre = lonLat(scale(toEye[2], -1));
  const cone = Math.min(89, Math.atan(Math.hypot(width / 2, height / 2) / f) / RAD + MARGIN);
  const least = Math.cos(cone * RAD);
  const lines: GridLine[] = [];
  const add = (coordinate: Coordinate, value: number, points: readonly (readonly [number, number])[]) => {
    const runs: [number, number][][] = [];
    let run: [number, number][] = [];
    for (const [lon, lat] of points) {
      const local: Vec3 = apply(toEye, directionOf(lon, lat));
      if (-local[2] >= least) run.push(pixelOf(f, local, width, height));
      else if (run.length > 0) {
        runs.push(run);
        run = [];
      }
    }
    runs.push(run);
    const drawn = runs.filter((r) => r.length > 1);
    if (drawn.length > 0) lines.push({ coordinate, value, runs: drawn });
  };
  const stepOf = (coordinate: Coordinate) => edges.find((e) => e.coordinate === coordinate)?.lineStep;
  const [cosC, sinC] = [Math.cos(centre.lat * RAD), Math.sin(centre.lat * RAD)];
  const lonStep = stepOf("longitude");
  if (lonStep !== undefined) {
    const reach = reaches(view, toFrame, width, height, edges);
    for (const lon of multiples(lonStep, 0, 360, true)) {
      // The angle from the line of sight to the half circle of this longitude: across to it
      // where it faces the line of sight, else to the nearer pole.
      const dLon = (lon - centre.lon) * RAD;
      const away = Math.cos(dLon) >= 0 ? Math.asin(cosC * Math.abs(Math.sin(dLon))) / RAD : 90 - Math.abs(centre.lat);
      if (away > cone) continue;
      const { south, north } = lon % THROUGH === 0 ? { south: 90, north: 90 } : (reach.get(lon) ?? { south: STOP, north: STOP });
      add("longitude", lon, samples(-south, north).map((lat) => [lon, lat] as const));
    }
  }
  const latStep = stepOf("latitude");
  if (latStep !== undefined) {
    for (const lat of multiples(latStep, -90, 90, false)) {
      if (Math.abs(lat - centre.lat) > cone) continue;
      // The longitudes either side of the line of sight's within the cone, or all round.
      const span = (least - Math.sin(lat * RAD) * sinC) / (Math.cos(lat * RAD) * cosC);
      const half = span <= -1 ? 180 : Math.acos(Math.min(1, span)) / RAD;
      add("latitude", lat, samples(centre.lon - half, centre.lon + half).map((lon) => [lon, lat] as const));
    }
  }
  return lines;
}
