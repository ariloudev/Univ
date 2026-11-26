// The Galaxy model's points (see-the-galaxy-model): drawn as light, each as bright as the next,
// in the display colour of its population; a point real objects cover hidden while the scale
// domain of its population's counterparts is drawn.

import { TRACERS_DOMAIN } from "./tracers.ts";

/** The scale domain of the Galaxy model, whose version 5 tiles hold model points, not objects. */
export const MODEL_DOMAIN = "galaxy_model";

/** The encoding version of the Galaxy model's tiles. */
export const MODEL_VERSION = 5;

/** The scale domain whose objects are the counterparts of the discs, the bar and bulge and the
 * halo. */
const STARS_DOMAIN = "stars";

/** What covers a model point: nothing, the stars domain's objects, or the young tracers of the
 * Galaxy tracers domain. */
export const NOT_COVERED = 0;
export const COVERED_BY_STARS = 1;
export const COVERED_BY_TRACERS = 2;

/** The opacity of a model point: one for every point, so that their light adds up where they
 * crowd, low enough that ten million of them do not wash the view out. */
export const MODEL_OPACITY = 0.35;

/** Bit `c` set for each cover `c` whose model points are hidden: those covered by the objects of
 * a scale domain the release holds, `held`, and that is drawn, not in `hidden`. */
export function coveredHiddenOf(hidden: ReadonlySet<string>, held: readonly string[]): number {
  let bits = 0;
  const drawn = (domain: string) => held.includes(domain) && !hidden.has(domain);
  if (drawn(STARS_DOMAIN)) bits |= 1 << COVERED_BY_STARS;
  if (drawn(TRACERS_DOMAIN)) bits |= 1 << COVERED_BY_TRACERS;
  return bits;
}

/** A model batch's points by what covers them, `NOT_COVERED`, `COVERED_BY_STARS` and
 * `COVERED_BY_TRACERS`: of those `inside` marks, where it marks any. */
export function coversOf(covered: Uint8Array, inside?: Uint8Array): [number, number, number] {
  const counts: [number, number, number] = [0, 0, 0];
  for (let i = 0; i < covered.length; i += 1) {
    if (inside !== undefined && inside[i] === 0) continue;
    const c = covered[i] ?? NOT_COVERED;
    counts[c] = (counts[c] ?? 0) + 1;
  }
  return counts;
}

/** Of a model batch's points counted by `covers`, those drawn: all but the covered ones
 * `hiddenBits` hides. */
export const modelPointsShown = (covers: readonly number[], hiddenBits: number): number =>
  covers.reduce((n, count, c) => (((hiddenBits >>> c) & 1) === 0 ? n + count : n), 0);

/** The opacity of a batch's points: its domain's, or that of a view culled to a volume, and of a
 * batch of the Galaxy model's light, `MODEL_OPACITY` of it. */
export const batchOpacity = (domainOpacity: number, light: boolean): number => domainOpacity * (light ? MODEL_OPACITY : 1);
