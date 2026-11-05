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
    { source: "mcconnachie-2012", identifier: "Andromeda" },
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


/** Betelgeuse as release 9 gives it, cut to what the explorer reads: placed by its star file. */
export const BETELGEUSE: ObjectRecord = {
  release: 9,
  id: 852881,
  kind: "body",
  entries: [
    { source: "hipparcos-1997", identifier: "27989", name: null, spectral_type: { text: "M2Ib", kind_of_value: "measured" } },
    { source: "hipparcos-2007", identifier: "27989", name: null, spectral_type: null },
    {
      source: "researched-star-distances",
      identifier: "27989",
      name: "Betelgeuse",
      spectral_type: null,
      references: [{ citation: "Joyce, Leung, Molnár, Ireland, Kobayashi & Nomoto 2020, ApJ 902, 63", bibcode: "2020ApJ...902...63J" }],
    },
  ],
  designations: [
    { source: "hipparcos-1997", identifier: "27989" },
    { source: "hipparcos-2007", identifier: "27989" },
    { source: "researched-star-distances", identifier: "27989" },
  ],
  measurements: [
    {
      entry: 1,
      source: "hipparcos-2007",
      quantity: "colour_index",
      n: 0,
      kind_of_value: "measured",
      native: { value: 1.5, unit: "magnitude", uncertainty: { form: "symmetric", sigma: 0.51 } },
      band: "B-V",
    },
    {
      entry: 2,
      source: "researched-star-distances",
      quantity: "distance",
      n: 0,
      kind_of_value: "estimated",
      native: { value: 168, unit: "parsec", uncertainty: { form: "interval", minus: 15, plus: 27 } },
      normalized: { value: 0.000168, unit: "megaparsec" },
    },
  ],
  preferred: {
    rule: "measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method",
    measurement: { entry: 2, quantity: "distance", n: 0 },
    distance_mpc: 0.000168,
    uncertainty: { form: "none_reported" },
  },
  position: { frame: "ICRS equatorial Cartesian", scale_domain: "stars", unit: "parsec", x: 4.4, y: 167.8, z: 21.7, epoch: 1991.25 },
  sky_position: { frame: "icrs", epoch: 1991.25 },
};

/** A star whose only type is Gaia's "unknown", with a BP−RP colour index: SIMULATED. */
export const UNTYPED_STAR: ObjectRecord = {
  ...BETELGEUSE,
  id: 1,
  entries: [{ source: "gaia-dr3-bright", identifier: "1", name: null, spectral_type: { text: "unknown", kind_of_value: "estimated" } }],
  designations: [{ source: "gaia-dr3-bright", identifier: "1" }],
  measurements: [
    {
      entry: 0,
      source: "gaia-dr3-bright",
      quantity: "colour_index",
      n: 0,
      kind_of_value: "measured",
      native: { value: 0.823, unit: "magnitude", uncertainty: { form: "none_reported" } },
      band: "BP-RP",
    },
  ],
};
