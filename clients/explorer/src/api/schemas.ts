// The parts of Univ Server's answers the explorer reads, checked as they arrive. Members the
// explorer does not read are let through unchecked.

import type { TileManifest } from "univ-tile-decoder";
import { z } from "zod";

const Cosmology = z.object({
  name: z.string(),
  hubble_constant: z.number(),
  omega_matter: z.number(),
});
/** A release's cosmology: its name, and the flat ΛCDM parameters its distances were made with. */
export type Cosmology = z.infer<typeof Cosmology>;

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

/** The scale domains of a release, by name. */
export const DomainList = z.object({ domains: z.array(z.object({ name: z.string() })) });

const Row = z.tuple([z.number(), z.number(), z.number()]);
const Matrix = z.tuple([Row, Row, Row]);

const Rotations = z.looseObject({
  to_galactic: Matrix,
  galactic_basis: z.string(),
  to_supergalactic: Matrix,
  supergalactic_basis: z.string(),
});

/** The rotations a tile manifest gives from the ICRS, row-major, each with its basis. */
export type Rotations = z.infer<typeof Rotations>;

/** The members of a tile manifest the explorer reads itself; the decoder checks the rest. */
const ManifestCheck = z.looseObject({
  unit: z.enum(["pc", "Mpc"]),
  rotations: Rotations,
  encoding: z.looseObject({
    kinds: z.array(z.enum(["measured", "estimated", "inferred", "reconstructed"])),
    /** The bands a point's colour index may lie between, by their code less one. */
    colour_bands: z.array(z.enum(["BP-RP", "B-V"])).optional(),
  }),
  sources: z.array(
    z.looseObject({ index: z.number().int(), id: z.string(), title: z.string(), citation: z.string(), credit: z.string() }),
  ),
  nodes: z.array(
    z.looseObject({
      address: z.string(),
      bounds: z.object({ min: Row, max: Row }),
      points: z.number().int(),
      sha256: z.string(),
    }),
  ),
});

/** A tile manifest as the explorer reads it: in parsecs or megaparsecs, with its rotations. */
export type ExplorerManifest = TileManifest & { unit: "pc" | "Mpc"; rotations: Rotations };

export const ExplorerManifest = z.custom<ExplorerManifest>(
  (value) => ManifestCheck.safeParse(value).success,
  "not a tile manifest the explorer can read: it needs a unit in pc or Mpc, the rotations, the kinds, the sources and the nodes",
);
