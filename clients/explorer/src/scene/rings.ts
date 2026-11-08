// The distance rings (frame-the-view-as-an-atlas): rings of equal distance from the Solar System
// barycentre in the equator of the chosen axes, at each multiple of the scale bar's length within
// reach of the focus, with spokes along every 30 degrees of longitude. They lie in space, so they
// move as the eye moves. Around a focus far from the barycentre only the arcs near it are drawn,
// finely, from a centre among them: a whole ring there would be drawn far off its circle.

import { type View, unitsPerPixel } from "./camera.ts";
import type { Lines } from "./drawing.ts";
import { type Axes, onEquator } from "./frames.ts";
import { shortDistance } from "./format.ts";
import { scaleBar } from "./scale.ts";
import { type Mat3, type Vec3, apply, sub } from "./vec.ts";

/** How far from the focus, in the plane, rings are drawn: this many times the eye's distance. */
const REACH = 3;

/** Degrees between the spokes. */
const SPOKES = 30;

/** Segments drawn round a whole ring, and along an arc. */
const RING_SEGMENTS = 192;
const ARC_SEGMENTS = 96;

/** The rings' colour: the dim ink, faint. */
const COLOUR = [0.557, 0.588, 0.671, 0.35] as const;

const RAD = Math.PI / 180;

/** The rings to draw: their step, megaparsecs; the first and last multiples of it; and, where
 * they are arcs, the longitude, degrees, their middle lies at and how far either side of it they
 * run. `key` tells one set from another. */
export interface RingSet {
  axes: Axes;
  step: number;
  first: number;
  last: number;
  sector: { lon: number; half: number } | undefined;
  key: string;
}

/** The rings for `view`, in the axes `axes` that `toFrame` turns the ICRS into, on a viewport
 * `height` CSS pixels tall; none where no ring comes within reach. The arcs' width is rounded up
 * to a power of two degrees and their middle to half of it, so that small moves keep the set. */
export function ringSet(view: View, toFrame: Mat3, axes: Axes, height: number): RingSet | undefined {
  const bar = scaleBar(unitsPerPixel(view, height));
  if (bar === undefined) return undefined;
  const step = bar.parsecs / 1e6;
  const [x, y] = apply(toFrame, view.focus);
  const rho = Math.hypot(x, y);
  const reach = REACH * view.distance;
  const first = Math.max(1, Math.ceil((rho - reach) / step - 1e-9));
  const last = Math.floor((rho + reach) / step + 1e-9);
  if (last < first) return undefined;
  let sector: RingSet["sector"];
  if (rho > reach) {
    const half = 2 ** Math.ceil(Math.log2(Math.asin(reach / rho) / RAD));
    const lon = Math.round(Math.atan2(y, x) / RAD / (half / 2)) * (half / 2);
    sector = { lon, half: Math.min(180, 1.5 * half) };
  }
  const key = `${axes}:${step}:${first}:${last}:${sector === undefined ? "round" : `${sector.lon}±${sector.half}`}`;
  return { axes, step, first, last, sector, key };
}

/** The ICRS point `radius` megaparsecs from the barycentre at longitude `lon` degrees in the
 * equator of the axes `toFrame` turns the ICRS into. */
const onPlane = (toFrame: Mat3, radius: number, lon: number): Vec3 => onEquator(toFrame, radius, lon * RAD);

/** The longitudes, degrees, a ring of `set` runs over, `segments` apart, both ends included. */
function sweep(set: RingSet): number[] {
  const [from, to, segments] = set.sector === undefined ? [0, 360, RING_SEGMENTS] : [set.sector.lon - set.sector.half, set.sector.lon + set.sector.half, ARC_SEGMENTS];
  return Array.from({ length: segments + 1 }, (_, i) => from + ((to - from) * i) / segments);
}

/** The longitudes of the spokes within `set`'s arcs, or all round. */
function spokes(set: RingSet): number[] {
  const all = Array.from({ length: 360 / SPOKES }, (_, i) => i * SPOKES);
  const { sector } = set;
  if (sector === undefined) return all;
  return all.filter((lon) => Math.abs(((lon - sector.lon + 540) % 360) - 180) <= sector.half);
}

/** The rings and spokes of `set` as ICRS lines, offset from a centre in the middle of them. */
export function ringLines(toFrame: Mat3, set: RingSet): Lines {
  const middle = set.sector === undefined ? 0 : ((set.first + set.last) / 2) * set.step;
  const centre = onPlane(toFrame, middle, set.sector?.lon ?? 0);
  const segments: Vec3[] = [];
  const longitudes = sweep(set);
  for (let k = set.first; k <= set.last; k += 1) {
    for (let i = 1; i < longitudes.length; i += 1) {
      segments.push(onPlane(toFrame, k * set.step, longitudes[i - 1] ?? 0), onPlane(toFrame, k * set.step, longitudes[i] ?? 0));
    }
  }
  for (const lon of spokes(set)) segments.push(onPlane(toFrame, set.first * set.step, lon), onPlane(toFrame, set.last * set.step, lon));
  const offsets = new Float32Array(segments.length * 3);
  segments.forEach((p, i) => offsets.set(sub(p, centre), i * 3));
  return { centre, offsets, colour: COLOUR };
}

/** Each ring's label: its distance, at its crossing with the spoke of zero longitude, or, for
 * arcs, with the line through their middle. */
export function ringLabels(toFrame: Mat3, set: RingSet): { text: string; at: Vec3 }[] {
  const lon = set.sector?.lon ?? 0;
  const labels: { text: string; at: Vec3 }[] = [];
  for (let k = set.first; k <= set.last; k += 1) labels.push({ text: shortDistance(k * set.step), at: onPlane(toFrame, k * set.step, lon) });
  return labels;
}
