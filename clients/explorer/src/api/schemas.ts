// The parts of Univ Server's answers the explorer reads, checked as they arrive. Members the
// explorer does not read are let through unchecked.

import { z } from "zod";

const Cosmology = z.object({
  name: z.string(),
  hubble_constant: z.number(),
  omega_matter: z.number(),
});

const ReleaseSummary = z.object({
  number: z.number().int(),
  published_at: z.string(),
  pruned_at: z.string().nullable().optional(),
  cosmology: Cosmology,
});
export type ReleaseSummary = z.infer<typeof ReleaseSummary>;

export const ReleaseList = z.object({
  latest: z.number().int(),
  releases: z.array(ReleaseSummary),
});
