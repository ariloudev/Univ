// Object records in the shape release 12 gives them, with designations in their catalogue's form,
// proper names, flags and relations. The values are those the local release 11 gave on
// 2026-11-08, cut to a few; the forms are those the source definitions give (HIP {identifier},
// Gaia DR3 {identifier}, Gaia EDR3 {identifier}); M42 is OpenNGC's NGC 1976 row, as release 12
// will hold it with no distance.

import type { ObjectRecord } from "../api/objects.ts";
import type { SourceDetail } from "../api/sources.ts";

const none = { form: "none_reported" } as const;

/** HIP 32754, a K dwarf 34.7 pc away, given by Hipparcos twice and by Gaia. */
export const HIP_32754: ObjectRecord = {
  release: 12,
  id: 17,
  kind: "body",
  entries: [
    { source: "bailer-jones-2021", identifier: "1000166358456525568", designation: "Gaia EDR3 1000166358456525568", name: null, flags: { flag: "10033" } },
    { source: "gaia-dr3-bright", identifier: "1000166358456525568", designation: "Gaia DR3 1000166358456525568", name: null, flags: {} },
    { source: "gaia-dr3-hipparcos2", identifier: "1000166358456525568", designation: "Gaia DR3 1000166358456525568", name: "32754", flags: { xm_flag: "8" } },
    { source: "hipparcos-1997", identifier: "32754", designation: "HIP 32754", name: null, flags: {} },
    { source: "hipparcos-2007", identifier: "32754", designation: "HIP 32754", name: null, flags: { Sn: "5" } },
  ],
  designations: [
    { source: "hipparcos-2007", identifier: "32754", designation: "HIP 32754" },
    { source: "hipparcos-1997", identifier: "32754", designation: "HIP 32754" },
    { source: "gaia-dr3-bright", identifier: "1000166358456525568", designation: "Gaia DR3 1000166358456525568" },
    { source: "gaia-dr3-hipparcos2", identifier: "1000166358456525568", designation: "Gaia DR3 1000166358456525568" },
    { source: "bailer-jones-2021", identifier: "1000166358456525568", designation: "Gaia EDR3 1000166358456525568" },
  ],
  names: [],
  relations: [],
  measurements: [
    {
      entry: 0,
      source: "bailer-jones-2021",
      quantity: "distance",
      n: 0,
      kind_of_value: "estimated",
      native: { value: 34.683907, unit: "parsec", uncertainty: { form: "credible_interval", minus: 0.014217, plus: 0.015277, probability: 0.68 } },
    },
    { entry: 1, source: "gaia-dr3-bright", quantity: "magnitude", n: 0, kind_of_value: "measured", native: { value: 9.878982, unit: "magnitude", uncertainty: none }, band: "phot_g_mean_mag" },
    { entry: 4, source: "hipparcos-2007", quantity: "magnitude", n: 0, kind_of_value: "measured", native: { value: 10.4996, unit: "magnitude", uncertainty: none }, band: "Hpmag" },
    { entry: 1, source: "gaia-dr3-bright", quantity: "proper_motion_ra", n: 0, kind_of_value: "measured", native: { value: -15.501847, unit: "milliarcsecond_per_year", uncertainty: none } },
    { entry: 1, source: "gaia-dr3-bright", quantity: "proper_motion_dec", n: 0, kind_of_value: "measured", native: { value: -221.079625, unit: "milliarcsecond_per_year", uncertainty: none } },
    {
      entry: 1,
      source: "gaia-dr3-bright",
      quantity: "velocity",
      n: 0,
      kind_of_value: "measured",
      native: { value: 27.252356, unit: "kilometre_per_second", uncertainty: { form: "symmetric", sigma: 0.19541511 } },
    },
  ],
  preferred: {
    rule: "measured-redshifts-first-then-indicators-over-uncertain-redshifts-then-uncertainty-then-method",
    measurement: { entry: 0, quantity: "distance", n: 0 },
    distance_mpc: 3.4683907e-5,
    uncertainty: { form: "reported", total: 0.000425 },
  },
  position: { frame: "ICRS equatorial Cartesian", scale_domain: "stars", unit: "parsec", x: -4.1965, y: 18.9965, z: 28.714, epoch: 2016 },
  sky_position: { frame: "icrs", ra: 102.4572103744033, dec: 55.88135624420838, epoch: 2016, entry: 1, uncertainty_mas: 0.010150478788455947 },
};

/** The meanings release 12's sources give the flags of HIP 32754, from their definitions. */
export const HIP_32754_SOURCES: SourceDetail[] = [
  { id: "hipparcos-2007", title: "Hipparcos, the new reduction", flags: { Sn: { values: { "5": "5-parameter solution; single star (d = 0, s = 5)." }, cite: "ReadMe of I/311, Note (1)" } } },
  {
    id: "gaia-dr3-hipparcos2",
    title: "Gaia DR3 best neighbours in Hipparcos-2",
    flags: { xm_flag: { values: { "8": "The Gaia object has a five parameters astrometric solution." }, cite: "Gaia DR3 documentation, 20.4.10" } },
  },
];

/** Andromeda, a galaxy that dominates its Cosmicflows-4 group, with its proper name. */
export const ANDROMEDA_GROUPED: ObjectRecord = {
  release: 12,
  id: 856254,
  kind: "body",
  entries: [
    { source: "updated-nearby-galaxies", identifier: "MESSIER031", name: null, flags: { "f_Dist": "Cep" } },
    { source: "cosmicflows-4-galaxies", identifier: "2557", designation: "PGC 2557", name: null, flags: {} },
  ],
  designations: [
    { source: "cosmicflows-4-galaxies", identifier: "2557", designation: "PGC 2557" },
    { source: "updated-nearby-galaxies", identifier: "MESSIER031" },
  ],
  names: [
    { name: "Andromeda Galaxy", source: "openngc" },
    { name: "Andromeda", source: "mcconnachie-2012" },
  ],
  relations: [
    { kind: "dominant_member_of", entry: { source: "cosmicflows-4-galaxies", identifier: "2557" }, other: { source: "cosmicflows-4", identifier: "2557" }, scheme: "pgc", identifier: "2557" },
    { kind: "member_of", entry: { source: "cosmicflows-4-galaxies", identifier: "2557" }, other: { source: "cosmicflows-4", identifier: "2557" }, scheme: "pgc", identifier: "2557" },
  ],
  measurements: [
    { entry: 0, source: "updated-nearby-galaxies", quantity: "magnitude", n: 0, kind_of_value: "measured", native: { value: 4.29, unit: "magnitude", uncertainty: none }, band: "Bmag" },
    { entry: 0, source: "updated-nearby-galaxies", quantity: "velocity", n: 0, kind_of_value: "measured", native: { value: -296, unit: "kilometre_per_second", uncertainty: none } },
    {
      entry: 1,
      source: "cosmicflows-4-galaxies",
      quantity: "redshift",
      n: 0,
      kind_of_value: "measured",
      native: { value: -585, unit: "kilometre_per_second", uncertainty: none },
      normalized: { value: -0.00195135, unit: "dimensionless" },
    },
  ],
  preferred: null,
  position: { frame: "ICRS equatorial Cartesian", scale_domain: "nearby_galaxies", unit: "megaparsec", x: 0.55, y: 0.1, z: 0.49, epoch: 2000 },
  sky_position: { frame: "icrs", ra: 10.685405460693017, dec: 41.269165540994514, epoch: 2000, entry: 0, uncertainty_mas: null },
};

/** M42, the Orion Nebula: named by OpenNGC, held by no catalogue with a distance. */
export const M42: ObjectRecord = {
  release: 12,
  id: 2000001,
  kind: "body",
  entries: [{ source: "openngc", identifier: "NGC 1976", designation: "NGC 1976", name: "Great Orion Nebula,Orion Nebula", flags: { type: "Cl+N" } }],
  designations: [
    { source: "openngc", identifier: "NGC 1976", designation: "NGC 1976" },
    { source: "openngc", identifier: "M 42", designation: "M 42" },
  ],
  names: [
    { name: "Great Orion Nebula", source: "openngc" },
    { name: "Orion Nebula", source: "openngc" },
  ],
  relations: [],
  measurements: [],
  preferred: null,
  position: null,
  not_placed: "no_distance",
  sky_position: { frame: "icrs", ra: 83.8186667, dec: -5.3896667, epoch: 2000, entry: 0, uncertainty_mas: null },
};
