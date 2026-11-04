// The axes the view shows (navigate-with-the-mouse): galactic, equatorial (the ICRS itself) or
// supergalactic, each a rotation of the ICRS that the release's tile manifests give, and the
// ring drawn around Earth in the chosen frame's equator.

import type { Lines } from "./renderer.ts";
import { IDENTITY, type Mat3, apply, transpose } from "./vec.ts";

export type Axes = "galactic" | "equatorial" | "supergalactic";

export const AXES: readonly Axes[] = ["galactic", "equatorial", "supergalactic"];

/** The rotations a tile manifest gives from the ICRS. */
export interface Rotations {
  to_galactic: Mat3;
  galactic_basis: string;
  to_supergalactic: Mat3;
  supergalactic_basis: string;
}

/** The rotation from the ICRS to `axes`, and the basis it is cited from. */
export function frameOf(axes: Axes, rotations: Rotations): { toFrame: Mat3; basis: string } {
  switch (axes) {
    case "galactic":
      return { toFrame: rotations.to_galactic, basis: rotations.galactic_basis };
    case "supergalactic":
      return { toFrame: rotations.to_supergalactic, basis: rotations.supergalactic_basis };
    case "equatorial":
      return { toFrame: IDENTITY, basis: "the ICRS itself" };
  }
}

/** Segments of the ring drawn round its circle. */
const SEGMENTS = 192;

/** The ring's radius for a view from `distance`: the power of ten nearest to it. */
export function ringRadius(distance: number): number {
  return 10 ** Math.round(Math.log10(distance));
}

/** The frame's equator as a ring of `radius` megaparsecs around Earth, as ICRS lines. */
export function equatorRing(toFrame: Mat3, radius: number): Lines {
  const back = transpose(toFrame);
  const offsets = new Float32Array(SEGMENTS * 6);
  for (let i = 0; i < SEGMENTS; i += 1) {
    for (const [j, k] of [
      [0, i],
      [1, i + 1],
    ] as const) {
      const angle = (2 * Math.PI * k) / SEGMENTS;
      const point = apply(back, [radius * Math.cos(angle), radius * Math.sin(angle), 0]);
      offsets.set(point, i * 6 + j * 3);
    }
  }
  return { centre: [0, 0, 0], offsets, colour: [0.49, 0.77, 1, 0.35] };
}
