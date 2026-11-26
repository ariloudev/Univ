// The Galaxy model's points (see-the-galaxy-model): drawn as light, each as bright as the next,
// in the display colour of its population; a point real objects cover hidden while the scale
// domain of its population's counterparts is drawn.

import { TRACERS_DOMAIN } from "./tracers.ts";

/** The scale domain of the Galaxy model, whose version 5 tiles hold model points, not objects. */
export const MODEL_DOMAIN = "galaxy_model";

/** The scale domain whose objects are the counterparts of the discs, the bar and bulge and the
 * halo. */
export const STARS_DOMAIN = "stars";

/** What covers a model point: nothing, the stars domain's objects, or the young tracers of the
 * Galaxy tracers domain. */
export const NOT_COVERED = 0;
export const COVERED_BY_STARS = 1;
export const COVERED_BY_TRACERS = 2;

/** What covers a covered model point, by the scale domain its population's manifest entry names
 * (`covered_by`): `NOT_COVERED` for a domain not listed, or none named. */
const COVERS: Readonly<Record<string, number>> = { [STARS_DOMAIN]: COVERED_BY_STARS, [TRACERS_DOMAIN]: COVERED_BY_TRACERS };

/** What covers a model point of a population whose manifest entry names `coveredBy`. */
export const coverOf = (coveredBy: string | undefined): number => COVERS[coveredBy ?? ""] ?? NOT_COVERED;

/** The opacity of the brightest model points, the arms' young stars and knots: the light of
 * every point adds up where they crowd, the old disc's low enough that ten million of them do
 * not wash the view out. */
export const MODEL_OPACITY = 0.48;

/** How many times as bright as a point of the discs, the bar and bulge and the halo the arms'
 * young stars and star-forming knots are drawn (see-the-galaxy-model), so that the arms stand
 * out as a spiral galaxy's do. */
export const ARM_BRIGHTNESS = 6;

/** The opacity byte of a population of density `form`: full for the arms' populations, a
 * sixth of it for any other. */
export const brightnessOf = (form: string): number => (form === "arms" ? 255 : Math.round(255 / ARM_BRIGHTNESS));

/** Bit `c` set for each cover `c` whose model points are hidden: those covered by the objects of
 * a scale domain the release holds, `held`, and that is drawn, not in `hidden`. */
export function coveredHiddenOf(hidden: ReadonlySet<string>, held: readonly string[]): number {
  let bits = 0;
  for (const [domain, cover] of Object.entries(COVERS)) {
    if (held.includes(domain) && !hidden.has(domain)) bits |= 1 << cover;
  }
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
