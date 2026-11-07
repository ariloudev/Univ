// The coordinates on the view's edges (frame-the-view-as-an-atlas): the direction of sight
// through each pixel of an edge, in the chosen axes, and a tick wherever it crosses a round
// longitude or latitude. A direction is the same from every point of space, so the ticks depend
// on the way the eye looks, not on where it is.

import type { View } from "./camera.ts";
import { eyeAxes, focalPixels } from "./camera.ts";
import type { Axes } from "./frames.ts";
import { type Mat3, type Vec3, apply, normalize, unapply } from "./vec.ts";

export type Edge = "top" | "right" | "bottom" | "left";
export type Coordinate = "longitude" | "latitude";

/** The steps between ticks, degrees, coarsest first: 90, 30, 15, 10, 5, 2 or 1 degrees, or a
 * tenth or a hundredth of one. */
const STEPS: readonly number[] = [90, 30, 15, 10, 5, 2, 1, 0.1, 0.01];

/** The least distance between two ticks' labels, CSS pixels. */
export const MIN_GAP = 60;

/** CSS pixels between the directions sampled along an edge. */
const SAMPLE = 2;

/** A change of longitude between two samples larger than this, degrees, passes by a pole: the
 * longitude jumps there rather than crossing the values between. */
const JUMP = 20;

/** A tick: how far along its edge, CSS pixels from its start (left or top), and its value. */
export interface Tick {
  at: number;
  value: number;
  label: string;
}

export interface EdgeTicks {
  edge: Edge;
  coordinate: Coordinate;
  /** The coordinate the ticks give, in words, such as "galactic longitude". */
  name: string;
  ticks: Tick[];
}

const NAMES: Record<Axes, Record<Coordinate, string>> = {
  equatorial: { longitude: "right ascension", latitude: "declination" },
  galactic: { longitude: "galactic longitude", latitude: "galactic latitude" },
  supergalactic: { longitude: "supergalactic longitude", latitude: "supergalactic latitude" },
};

/** The coordinate each edge gives: longitude along the top and bottom, latitude up the sides. */
const EDGES: readonly (readonly [Edge, Coordinate])[] = [
  ["top", "longitude"],
  ["bottom", "longitude"],
  ["left", "latitude"],
  ["right", "latitude"],
];

/** The direction of sight through the pixel (`x`, `y`) of a viewport `width` × `height` CSS
 * pixels, a unit vector in the axes of `toFrame`. */
function directionThrough(view: View, toFrame: Mat3, x: number, y: number, width: number, height: number): Vec3 {
  const f = focalPixels(height);
  const eye: Vec3 = [(x - width / 2) / f, -(y - height / 2) / f, -1];
  return normalize(apply(toFrame, unapply(eyeAxes(view, toFrame), eye)));
}

/** The longitude, 0 to 360, and latitude, -90 to 90, of the direction `d`, degrees. */
export function lonLat(d: Vec3): { lon: number; lat: number } {
  const lon = (Math.atan2(d[1], d[0]) * 180) / Math.PI;
  const lat = (Math.asin(Math.max(-1, Math.min(1, d[2]))) * 180) / Math.PI;
  return { lon: lon < 0 ? lon + 360 : lon, lat };
}

/** `value` without the error of the arithmetic that stepped to it. */
const tidy = (value: number): number => Math.round(value * 1e6) / 1e6;

/** Degrees, to two decimals at most. */
const degrees = (value: number): string => `${tidy(Math.round(value * 100) / 100).toLocaleString("en-GB", { maximumFractionDigits: 2 })}°`;

/** A tick's label: right ascension in hours and minutes; any other longitude in degrees, 0 to
 * 360; a latitude in degrees with its sign. */
export function tickLabel(value: number, coordinate: Coordinate, axes: Axes): string {
  if (coordinate === "latitude") {
    const text = degrees(Math.abs(value));
    return value > 0 ? `+${text}` : value < 0 ? `−${text}` : text;
  }
  const lon = ((value % 360) + 360) % 360;
  if (axes !== "equatorial") return degrees(lon);
  const minutesOfDay = tidy((lon / 15) * 60);
  const hours = Math.floor(minutesOfDay / 60);
  const minutes = tidy(minutesOfDay - hours * 60);
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes.toLocaleString("en-GB", { maximumFractionDigits: 2 })}m`;
}

/** The samples along an edge: how far along it, and the coordinate there. */
type Samples = readonly (readonly [number, number])[];

/** The change of a coordinate from `v1` to `v2`: of a longitude, the shorter way round. */
function change(v1: number, v2: number, longitude: boolean): number {
  const delta = v2 - v1;
  return longitude ? ((((delta + 540) % 360) + 360) % 360) - 180 : delta;
}

/** The multiples of `step` the coordinate crosses from `v1` at `p1` by `delta` to `p2`, each
 * where it is crossed; one met exactly at the end is counted there, and not again at the start of
 * the next. */
function between(p1: number, p2: number, v1: number, delta: number, step: number, longitude: boolean): { at: number; value: number }[] {
  const [low, high] = delta > 0 ? [v1, v1 + delta] : [v1 + delta, v1];
  const found: { at: number; value: number }[] = [];
  for (let k = Math.floor(low / step) + 1; k * step <= high; k += 1) {
    const value = tidy(k * step);
    found.push({ at: p1 + ((value - v1) / delta) * (p2 - p1), value: longitude ? tidy(((value % 360) + 360) % 360) : value });
  }
  return found;
}

/** The crossings of multiples of `step` between successive `samples`; none where there would be
 * more than `cap`, as so many ticks cannot keep their gap. A longitude that jumps between two
 * samples passes by a pole, and crosses nothing there. */
function crossings(samples: Samples, step: number, longitude: boolean, cap: number): { at: number; value: number }[] | undefined {
  const found: { at: number; value: number }[] = [];
  for (let i = 1; i < samples.length; i += 1) {
    const [p1, v1] = samples[i - 1] ?? [0, 0];
    const [p2, v2] = samples[i] ?? [0, 0];
    const delta = change(v1, v2, longitude);
    if (delta === 0 || (longitude && Math.abs(delta) > JUMP)) continue;
    found.push(...between(p1, p2, v1, delta, step, longitude));
    if (found.length > cap) return undefined;
  }
  return found;
}

/** The least distance between successive ticks; infinite for fewer than two. */
function leastGap(ticks: readonly { at: number }[]): number {
  const at = ticks.map((t) => t.at).sort((a, b) => a - b);
  let gap = Number.POSITIVE_INFINITY;
  for (let i = 1; i < at.length; i += 1) gap = Math.min(gap, (at[i] ?? 0) - (at[i - 1] ?? 0));
  return gap;
}

/** The ticks of the finest step of `STEPS` whose ticks lie at least `MIN_GAP` apart along an
 * edge `length` pixels long; none where no step does. */
export function ticksOf(samples: Samples, length: number, longitude: boolean): { step: number; ticks: { at: number; value: number }[] } | undefined {
  // Ticks closer than the gap cannot number more than this along the edge.
  const cap = Math.floor(length / MIN_GAP) + 1;
  let best: { step: number; ticks: { at: number; value: number }[] } | undefined;
  for (const step of STEPS) {
    const ticks = crossings(samples, step, longitude, cap);
    if (ticks !== undefined && leastGap(ticks) >= MIN_GAP) best = { step, ticks };
  }
  return best;
}

/** The ticks of each edge of a viewport `width` × `height` CSS pixels, for `view` in the axes
 * `axes`, which `toFrame` turns the ICRS into. */
export function edgeTicks(view: View, toFrame: Mat3, axes: Axes, width: number, height: number): EdgeTicks[] {
  return EDGES.map(([edge, coordinate]) => {
    const along = edge === "top" || edge === "bottom";
    const length = along ? width : height;
    const count = Math.max(1, Math.ceil(length / SAMPLE));
    const samples: [number, number][] = [];
    for (let i = 0; i <= count; i += 1) {
      const p = (i * length) / count;
      const [x, y] = along ? [p, edge === "top" ? 0 : height] : [edge === "left" ? 0 : width, p];
      const { lon, lat } = lonLat(directionThrough(view, toFrame, x, y, width, height));
      samples.push([p, coordinate === "longitude" ? lon : lat]);
    }
    const found = ticksOf(samples, length, coordinate === "longitude");
    const ticks = (found?.ticks ?? []).map((t) => ({ ...t, label: tickLabel(t.value, coordinate, axes) }));
    return { edge, coordinate, name: NAMES[axes][coordinate], ticks };
  });
}
