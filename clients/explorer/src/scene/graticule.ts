// The coordinates on the view's edges (frame-the-view-as-an-atlas): the direction of sight
// through each pixel of an edge, in the chosen axes, and a tick wherever it crosses a round
// longitude or latitude. A direction is the same from every point of space, so the ticks depend
// on the way the eye looks, not on where it is.

import type { View } from "./camera.ts";
import { eyeAxes, focalPixels, pixelRay } from "./camera.ts";
import type { Axes } from "./frames.ts";
import { type Mat3, type Vec3, apply, multiply, normalize, transpose } from "./vec.ts";

export type Edge = "top" | "right" | "bottom" | "left";
type Coordinate = "longitude" | "latitude";

/** The steps between ticks, degrees, coarsest first: 90, 30, 15, 10, 5, 2 or 1 degrees, or a
 * tenth or a hundredth of one. */
const STEPS: readonly number[] = [90, 30, 15, 10, 5, 2, 1, 0.1, 0.01];

/** The steps between ticks of right ascension, in degrees, at 15 to the hour: 6, 2 or 1 hours;
 * 30, 20, 10, 5, 2 or 1 minutes; or a tenth or a hundredth of a minute. */
const RA_STEPS: readonly number[] = [90, 30, 15, 7.5, 5, 2.5, 1.25, 0.5, 0.25, 0.025, 0.0025];

/** The least distance between two ticks' labels, CSS pixels. */
export const MIN_GAP = 60;

/** CSS pixels between the directions sampled along an edge. */
const SAMPLE = 2;

/** A change of longitude between two samples larger than this, degrees, passes by a pole: the
 * longitude jumps there rather than crossing the values between. */
const JUMP = 20;

/** Where a coordinate crosses a round value: how far along its edge, CSS pixels from its start
 * (left or top), and the value. */
interface Crossing {
  at: number;
  value: number;
}

export interface EdgeTicks {
  edge: Edge;
  /** The coordinate the ticks give, in words, such as "galactic longitude". */
  name: string;
  ticks: (Crossing & { label: string })[];
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

/** Whether `edge` runs across the view rather than up it. */
export const across = (edge: Edge): boolean => edge === "top" || edge === "bottom";

/** The pixel `at` along `edge` of a viewport `width` × `height`, `inset` pixels inside it. */
export function edgePoint(edge: Edge, at: number, inset: number, width: number, height: number): [number, number] {
  switch (edge) {
    case "top":
      return [at, inset];
    case "bottom":
      return [at, height - inset];
    case "left":
      return [inset, at];
    case "right":
      return [width - inset, at];
  }
}

/** The longitude, 0 to 360, and latitude, -90 to 90, of the direction `d`, degrees. */
export function lonLat(d: Vec3): { lon: number; lat: number } {
  const lon = (Math.atan2(d[1], d[0]) * 180) / Math.PI;
  const lat = (Math.asin(Math.max(-1, Math.min(1, d[2]))) * 180) / Math.PI;
  return { lon: wrap360(lon), lat };
}

/** `degrees` within 0 to 360. */
const wrap360 = (degrees: number): number => ((degrees % 360) + 360) % 360;

/** `value` without the error of the arithmetic that stepped to it. */
const tidy = (value: number): number => Math.round(value * 1e6) / 1e6;

/** Up to two decimals, made once: a format made for each label would cost more than the rest of
 * the frame's ticks. */
const TWO_PLACES = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 2 });

/** A tick's label: right ascension in hours and minutes; any other longitude in degrees, 0 to
 * 360; a latitude in degrees with its sign. */
export function tickLabel(value: number, coordinate: Coordinate, axes: Axes): string {
  if (coordinate === "latitude") {
    const text = `${TWO_PLACES.format(Math.abs(value))}°`;
    return value > 0 ? `+${text}` : value < 0 ? `−${text}` : text;
  }
  const lon = wrap360(value);
  if (axes !== "equatorial") return `${TWO_PLACES.format(lon)}°`;
  const minutesOfDay = tidy((lon / 15) * 60);
  const hours = Math.floor(minutesOfDay / 60);
  const minutes = tidy(minutesOfDay - hours * 60);
  return minutes === 0 ? `${hours}h` : `${hours}h ${TWO_PLACES.format(minutes)}m`;
}

/** The samples along an edge: how far along it, and the coordinate there. */
type Samples = readonly (readonly [number, number])[];

/** The change of a coordinate from `v1` to `v2`: of a longitude, the shorter way round. */
const change = (v1: number, v2: number, longitude: boolean): number => (longitude ? wrap360(v2 - v1 + 180) - 180 : v2 - v1);

/** The crossings of multiples of `step` between successive `samples`, added to `found`; false
 * where there would be more than `cap`, as so many ticks cannot keep their gap. A longitude that
 * jumps between two samples passes by a pole, and crosses nothing there. A multiple met exactly
 * at a sample is counted at the segment it ends. */
function crossings(samples: Samples, step: number, longitude: boolean, cap: number, found: Crossing[]): boolean {
  for (let i = 1; i < samples.length && found.length <= cap; i += 1) {
    const [p1, v1] = samples[i - 1] ?? [0, 0];
    const [p2, v2] = samples[i] ?? [0, 0];
    const delta = change(v1, v2, longitude);
    if (delta !== 0 && !(longitude && Math.abs(delta) > JUMP)) between(found, [p1, p2], v1, delta, step, longitude);
  }
  return found.length <= cap;
}

/** Adds to `found` each multiple of `step` the coordinate crosses from `v1` by `delta` between the
 * positions `span`, where it crosses it. */
function between(found: Crossing[], [p1, p2]: readonly [number, number], v1: number, delta: number, step: number, longitude: boolean): void {
  const [low, high] = delta > 0 ? [v1, v1 + delta] : [v1 + delta, v1];
  for (let k = Math.floor(low / step) + 1; k * step <= high; k += 1) {
    const value = tidy(k * step);
    found.push({ at: p1 + ((value - v1) / delta) * (p2 - p1), value: longitude ? tidy(wrap360(value)) : value });
  }
}

/** The least distance between successive ticks; infinite for fewer than two. */
function leastGap(ticks: readonly Crossing[]): number {
  const at = ticks.map((t) => t.at).sort((a, b) => a - b);
  let gap = Number.POSITIVE_INFINITY;
  for (let i = 1; i < at.length; i += 1) gap = Math.min(gap, (at[i] ?? 0) - (at[i - 1] ?? 0));
  return gap;
}

/** The ticks of the finest of `steps` whose ticks lie at least `MIN_GAP` apart along an edge
 * `length` pixels long; none where no step does. Every step is tried, since the steps do not all
 * divide one another: 10 can fit where 15 does not. */
export function ticksOf(samples: Samples, length: number, longitude: boolean, steps: readonly number[] = STEPS): { step: number; ticks: Crossing[] } | undefined {
  // Ticks closer than the gap cannot number more than this along the edge.
  const cap = Math.floor(length / MIN_GAP) + 1;
  let best: { step: number; ticks: Crossing[] } | undefined;
  for (const step of steps) {
    const ticks: Crossing[] = [];
    if (crossings(samples, step, longitude, cap, ticks) && leastGap(ticks) >= MIN_GAP) best = { step, ticks };
  }
  return best;
}

/** The ticks of each edge of a viewport `width` × `height` CSS pixels, for `view` in the axes
 * `axes`, which `toFrame` turns the ICRS into. */
export function edgeTicks(view: View, toFrame: Mat3, axes: Axes, width: number, height: number): EdgeTicks[] {
  const f = focalPixels(height);
  // From the eye's axes to the chosen axes, through the ICRS.
  const toAxes = multiply(toFrame, transpose(eyeAxes(view, toFrame)));
  return EDGES.map(([edge, coordinate]) => {
    const length = across(edge) ? width : height;
    const count = Math.max(1, Math.ceil(length / SAMPLE));
    const samples: [number, number][] = [];
    for (let i = 0; i <= count; i += 1) {
      const p = (i * length) / count;
      const [x, y] = edgePoint(edge, p, 0, width, height);
      const { lon, lat } = lonLat(normalize(apply(toAxes, pixelRay(f, x, y, width, height))));
      samples.push([p, coordinate === "longitude" ? lon : lat]);
    }
    const longitude = coordinate === "longitude";
    const found = ticksOf(samples, length, longitude, longitude && axes === "equatorial" ? RA_STEPS : STEPS);
    const ticks = (found?.ticks ?? []).map((t) => ({ ...t, label: tickLabel(t.value, coordinate, axes) }));
    return { edge, name: NAMES[axes][coordinate], ticks };
  });
}
