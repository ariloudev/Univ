// The stars seen from across the Galaxy (draw-the-catalogues): a million and more crowd about the
// Sun, so beyond a kiloparsec from the focus they grow fainter as the eye withdraws, rather than
// adding up to white.

import type { Detail } from "../catalogue/lod.ts";
import { detailOpacity } from "./kinds.ts";
import { STARS_DOMAIN } from "./model.ts";

/** Beyond this distance from the focus, megaparsecs, a kiloparsec, the stars grow fainter. */
const STARS_FADE_FROM = 0.001;
/** The faintest they grow: a tenth. */
const STARS_FADE_FLOOR = 0.1;

/** How much of their opacity the stars keep with the eye `distance` from the focus, megaparsecs:
 * all within a kiloparsec, then in inverse proportion to the distance, down to a tenth. */
export const starsFade = (distance: number): number => Math.max(STARS_FADE_FLOOR, Math.min(1, STARS_FADE_FROM / distance));

/** The opacity `domain`'s points are drawn at with its level of detail `detail`, the eye `distance`
 * from the focus: `detailOpacity`, and for the stars `starsFade` of it. */
export const domainOpacity = (domain: string, detail: Detail, distance: number): number =>
  detailOpacity(detail, distance) * (domain === STARS_DOMAIN ? starsFade(distance) : 1);
