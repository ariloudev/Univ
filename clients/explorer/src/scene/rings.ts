// The distance rings (frame-the-view-as-an-atlas): rings of equal distance from the Solar System
// barycentre in the equator of the chosen axes, at each multiple of the scale bar's length within
// reach of the focus, with spokes along every 30 degrees of longitude. They lie in space, so they
// move as the eye moves. Around a focus far from the barycentre only the arcs near it are drawn,
// finely, from a centre among them: a whole ring there would be drawn far off its circle.

import { type View, unitsPerPixel } from "./camera.ts";
import type { Lines } from "./drawing.ts";
import { type Axes, onEquator } from "./frames.ts";
import { scaleBar } from "./scale.ts";
import { PC_UNITS } from "./format.ts";
import { type Mat3, RAD, type Vec3, apply, norm, scale } from "./vec.ts";

/** How far from the focus, in the plane, rings must reach, may run on to, and are drawn to when
 * drawn again: these many times the eye's distance. A set drawn is kept while it reaches the one
 * and runs no farther than the other, so that a moving view draws it again only now and then. */
const REACH = 3;
const RUN_ON = 4;
const DRAWN = 3.5;

/** Degrees between the spokes. */
const SPOKES = 30;

/** Segments drawn round a whole ring, and along an arc. */
const RING_SEGMENTS = 192;
const ARC_SEGMENTS = 96;

/** How far apart on the view, CSS pixels, rings `step` megaparsecs apart lie at `point`, a point
 * of one in the ICRS, by where `at` places half a step either side of it out from the
 * barycentre; unbounded where either is out of sight. */
export function ringSpacing(at: (p: Vec3) => { x: number; y: number } | undefined, point: Vec3, step: number): number {
  const h = step / (2 * norm(point));
  const inner = at(scale(point, 1 - h));
  const outer = at(scale(point, 1 + h));
  return inner === undefined || outer === undefined ? Infinity : Math.hypot(outer.x - inner.x, outer.y - inner.y);
}

/** The rings' colour: the dim ink, faint. */
const COLOUR = [0.557, 0.588, 0.671, 0.35] as const;

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

/** A ring's label: its distance, and the ICRS point it is placed at. */
export interface RingLabel {
  text: string;
  at: Vec3;
}

/** The multiples of `step` whose rings pass within `reach` of `rho`, the focus's distance in the
 * plane: the first and the last. */
const within = (rho: number, reach: number, step: number): [number, number] => [Math.max(1, Math.ceil((rho - reach) / step - 1e-9)), Math.floor((rho + reach) / step + 1e-9)];

/** The arcs' middle and half-width for a focus at longitude `lon`, degrees, `rho` from the
 * barycentre in the plane, rings running on `reach` from it; none where rings go round. The
 * width is rounded up to a power of two degrees and the middle to half of it, so that small moves
 * keep them. */
function sectorOf(rho: number, reach: number, lon: number): RingSet["sector"] {
  if (rho <= reach) return undefined;
  const half = 2 ** Math.ceil(Math.log2(Math.asin(reach / rho) / RAD));
  // Arcs that would go round are whole rings.
  if (1.5 * half >= 180) return undefined;
  return { lon: Math.round(lon / (half / 2)) * (half / 2), half: 1.5 * half };
}

/** The rings for `view`, in the axes `axes` that `toFrame` turns the ICRS into, on a viewport
 * `height` CSS pixels tall; none where no ring comes within reach. `kept`, the set drawn, is kept
 * while it still serves. */
export function ringSet(view: View, toFrame: Mat3, axes: Axes, height: number, kept?: RingSet): RingSet | undefined {
  const bar = scaleBar(unitsPerPixel(view, height));
  if (bar === undefined) return undefined;
  const step = bar.parsecs / 1e6;
  const [x, y] = apply(toFrame, view.focus);
  const rho = Math.hypot(x, y);
  const d = view.distance;
  const [needFirst, needLast] = within(rho, REACH * d, step);
  if (needLast < needFirst) return undefined;
  const sector = sectorOf(rho, RUN_ON * d, Math.atan2(y, x) / RAD);
  const where = `${axes}:${step}:${sector === undefined ? "round" : `${sector.lon}±${sector.half}`}`;
  const [mayFirst, mayLast] = within(rho, RUN_ON * d, step);
  if (kept !== undefined && kept.key.startsWith(`${where}:`) && kept.first <= needFirst && kept.last >= needLast && kept.first >= mayFirst && kept.last <= mayLast) return kept;
  const [first, last] = within(rho, DRAWN * d, step);
  return { axes, step, first, last, sector, key: `${where}:${first}:${last}` };
}

/** The ICRS direction at longitude `lon` degrees in the equator of the axes `toFrame` turns the
 * ICRS into. */
const along = (toFrame: Mat3, lon: number): Vec3 => onEquator(toFrame, 1, lon * RAD);

/** The longitudes, degrees, a ring of `set` runs over, both ends included. */
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

/** The rings and spokes of `set` as ICRS lines, offset from a centre in the middle of them; each
 * direction worked out once. Each ring's ends carry the vector out to the next ring, by which it
 * fades as the rings crowd on the view; the spokes' carry none, and they do not fade. */
export function ringLines(toFrame: Mat3, set: RingSet): Lines {
  const middle = set.sector === undefined ? 0 : ((set.first + set.last) / 2) * set.step;
  const centre = scale(along(toFrame, set.sector?.lon ?? 0), middle);
  const round = sweep(set).map((lon) => along(toFrame, lon));
  const offsets: number[] = [];
  const neighbours: number[] = [];
  const put = (direction: Vec3, radius: number, next: Vec3) => {
    offsets.push(...direction.map((d, c) => d * radius - (centre[c] ?? 0)));
    neighbours.push(...next);
  };
  for (let k = set.first; k <= set.last; k += 1) {
    for (let i = 1; i < round.length; i += 1) {
      const [a = centre, b = centre] = [round[i - 1], round[i]];
      put(a, k * set.step, scale(a, set.step));
      put(b, k * set.step, scale(b, set.step));
    }
  }
  for (const direction of spokes(set).map((lon) => along(toFrame, lon))) {
    put(direction, set.first * set.step, NONE);
    put(direction, set.last * set.step, NONE);
  }
  return { centre, offsets: Float32Array.from(offsets), neighbours: Float32Array.from(neighbours), colour: COLOUR };
}

/** No neighbour: a line that does not fade. */
const NONE: Vec3 = [0, 0, 0];

/** Each ring's label: its distance, at its crossing with the spoke of zero longitude, or, for
 * arcs, with the line through their middle. */
export function ringLabels(toFrame: Mat3, set: RingSet): RingLabel[] {
  const direction = along(toFrame, set.sector?.lon ?? 0);
  const labels: RingLabel[] = [];
  const text = distanceOf(set.step);
  for (let k = set.first; k <= set.last; k += 1) labels.push({ text: text(k), at: scale(direction, k * set.step) });
  return labels;
}

/** The distance of the `k`th ring `step` megaparsecs apart, in the largest parsec unit of which
 * it is at least one, to the step's last figure in that unit, so that rings side by side never
 * read alike: 800 kpc before 1.0 Mpc, 1.005 Gpc beside 1.010 Gpc. */
function distanceOf(step: number): (k: number) => string {
  const inUnit = ([size, unit]: readonly [number, string]) => {
    const places = Math.max(0, -Math.floor(Math.log10((step * 1e6) / size) + 1e-9));
    return { size, unit, format: new Intl.NumberFormat("en-GB", { minimumFractionDigits: places, maximumFractionDigits: places }) };
  };
  const formats = PC_UNITS.map(inUnit);
  const parsecs = inUnit([1, "pc"]);
  return (k) => {
    const pc = k * step * 1e6;
    const { size, unit, format } = formats.find((f) => pc >= f.size * (1 - 1e-9)) ?? parsecs;
    return `${format.format(pc / size)} ${unit}`;
  };
}
