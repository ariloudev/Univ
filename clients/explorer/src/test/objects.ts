// An object record as release 7 gives it, cut to what the explorer reads.

import type { ObjectRecord } from "../api/objects.ts";

/** Andromeda as release 7 gives it, cut to what the explorer reads. */
export const ANDROMEDA: ObjectRecord = {
  release: 7,
  id: 856254,
  kind: "body",
  entries: [
    { source: "cosmicflows-4-galaxies", identifier: "2557", name: null },
    { source: "mcconnachie-2012", identifier: "Andromeda", name: null },
  ],
  designations: [
    { source: "cosmicflows-4-galaxies", identifier: "2557" },
    { source: "updated-nearby-galaxies", identifier: "MESSIER031" },
  ],
  measurements: [
    {
      entry: 0,
      source: "cosmicflows-4-galaxies",
      quantity: "distance",
      n: 0,
      kind_of_value: "measured",
      native: { value: 24.366, unit: "magnitude", uncertainty: { form: "symmetric", sigma: 0.045 } },
      normalized: { value: 0.7468, unit: "megaparsec" },
    },
    {
      entry: 0,
      source: "cosmicflows-4-galaxies",
      quantity: "redshift",
      n: 0,
      kind_of_value: "measured",
      native: { value: -585, unit: "kilometre_per_second" },
    },
    {
      entry: 1,
      source: "mcconnachie-2012",
      quantity: "distance",
      n: 0,
      kind_of_value: "measured",
      native: { value: 783, unit: "kiloparsec" },
    },
  ],
  preferred: {
    rule: "measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method",
    measurement: { entry: 0, quantity: "distance", n: 0 },
    distance_mpc: 0.7468,
    uncertainty: { form: "reported", total: 0.0207 },
  },
  position: { frame: "ICRS equatorial Cartesian", scale_domain: "nearby_galaxies", unit: "megaparsec", x: 0.55, y: 0.1, z: 0.49, epoch: 2000 },
  sky_position: { frame: "icrs", epoch: 2000 },
};

