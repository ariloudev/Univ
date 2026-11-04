// The axes the view shows (navigate-with-the-mouse): galactic, equatorial (the ICRS itself) or
// supergalactic, each a rotation of the ICRS that the release's tile manifests give, and the
// ring drawn around Earth in the chosen frame's equator.

import type { Rotations } from "../api/schemas.ts";
import type { Lines } from "./drawing.ts";
import { IDENTITY, type Mat3, ORIGIN, type Vec3, unapply } from "./vec.ts";

export type Axes = "galactic" | "equatorial" | "supergalactic";

export const AXES: readonly Axes[] = ["galactic", "equatorial", "supergalactic"];

/** The axes of a frame: the rotation from the ICRS, and the basis it is cited from. */
export interface Frame {
  toFrame: Mat3;
  basis: string;
}

/** The ICRS's own axes, which need no rotation. */
export const EQUATORIAL: Frame = { toFrame: IDENTITY, basis: "the ICRS itself" };

/** The frame of `axes`; none for a rotated frame while the release's rotations are unknown. */
export function frameOf(axes: Axes, rotations: Rotations | undefined): Frame | undefined {
  if (axes === "equatorial") return EQUATORIAL;
  if (rotations === undefined) return undefined;
  return axes === "galactic"
    ? { toFrame: rotations.to_galactic, basis: rotations.galactic_basis }
    : { toFrame: rotations.to_supergalactic, basis: rotations.supergalactic_basis };
}

/** Segments of the ring drawn round its circle. */
const SEGMENTS = 192;

/** The ring's radius for a view from `distance`: the power of ten nearest to it. */
export function ringRadius(distance: number): number {
  return 10 ** Math.round(Math.log10(distance));
}

/** The ICRS point at `angle` radians round the frame's equator, `radius` from Earth. */
function onEquator(toFrame: Mat3, radius: number, angle: number): Vec3 {
  return unapply(toFrame, [radius * Math.cos(angle), radius * Math.sin(angle), 0]);
}

/** Where the ring's label sits: its point at zero longitude. */
export const ringAnchor = (toFrame: Mat3, radius: number): Vec3 => onEquator(toFrame, radius, 0);

/** The frame's equator as a ring of `radius` megaparsecs around Earth, as ICRS lines. */
export function equatorRing(toFrame: Mat3, radius: number): Lines {
  const offsets = new Float32Array(SEGMENTS * 6);
  const step = (2 * Math.PI) / SEGMENTS;
  for (let i = 0; i < SEGMENTS; i += 1) {
    offsets.set(onEquator(toFrame, radius, i * step), i * 6);
    offsets.set(onEquator(toFrame, radius, (i + 1) * step), i * 6 + 3);
  }
  return { centre: ORIGIN, offsets, colour: [0.49, 0.77, 1, 0.35] };
}
