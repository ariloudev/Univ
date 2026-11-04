// Univ Server's structures as the explorer reads them (see-the-structures), checked as they
// arrive. Members the explorer does not read are let through unchecked.

import { z } from "zod";

import { type ReadOptions, readJson } from "./client.ts";

const Triple = z.tuple([z.number(), z.number(), z.number()]);

const Centre = z.looseObject({ x: z.number(), y: z.number(), z: z.number(), unit: z.string() });

const Summary = z.looseObject({ id: z.string(), name: z.string(), kind: z.string(), scale_domain: z.string() });
export type StructureSummary = z.infer<typeof Summary>;

const Page = z.looseObject({ structures: z.array(Summary), next: z.string().nullable().optional() });

const Primitive = z.discriminatedUnion("form", [
  z.looseObject({ form: z.literal("sphere"), centre: Triple, radius: z.number() }),
  z.looseObject({ form: z.literal("ellipsoid"), centre: Triple, semi_axes: Triple, orientation: z.tuple([Triple, Triple, Triple]) }),
  z.looseObject({ form: z.literal("oriented_box"), centre: Triple, half_extents: Triple, orientation: z.tuple([Triple, Triple, Triple]) }),
]);

const Geometry = z.discriminatedUnion("kind", [
  z.looseObject({ kind: z.literal("point"), at: Triple }),
  z.looseObject({ kind: z.literal("ray"), origin: Triple, direction: Triple, reach: z.number() }),
  z.looseObject({ kind: z.literal("range"), direction: Triple, near: z.number(), far: z.number() }),
  z.looseObject({ kind: z.literal("primitive"), primitive: Primitive }),
  z.looseObject({ kind: z.literal("path"), points: z.array(Triple) }),
  z.looseObject({ kind: z.literal("members"), members: z.array(z.looseObject({ at: Triple })) }),
  z.looseObject({ kind: z.literal("sky_region"), order: z.number().int(), cells: z.array(z.number().int()), near: z.number(), far: z.number() }),
  z.looseObject({ kind: z.literal("volume_grid") }),
  z.looseObject({ kind: z.literal("grid_region") }),
]);
export type Geometry = z.infer<typeof Geometry>;

const Shape = z.looseObject({ frame: z.string(), unit: z.string(), geometry: Geometry });

const Cited = z.looseObject({
  kind: z.string().optional(),
  quantity: z.looseObject({ value: z.unknown(), unit: z.string().optional(), uncertainty: z.unknown().optional() }).optional(),
  cite: z.looseObject({ paper: z.string() }).nullable().optional(),
});

const Paper = z.looseObject({ id: z.string(), reference: z.string(), title: z.string().optional() });

const StructureDetail = z.looseObject({
  id: z.string(),
  name: z.string(),
  kind: z.string(),
  status: z.string().optional(),
  scale_domain: z.string(),
  centre: Centre.nullable(),
  bounds: z.looseObject({ min: Triple, max: Triple }).nullable(),
  shape: Shape.nullable(),
  derivation: z.looseObject({ from: z.array(z.string()).optional() }).nullable().optional(),
  record: z.looseObject({
    papers: z.array(Paper).optional(),
    values: z.record(z.string(), Cited).optional(),
  }),
});
export type StructureDetail = z.infer<typeof StructureDetail>;

/** Every structure of `release`, page after page. */
export async function structuresOf(release: number, options: ReadOptions = {}): Promise<StructureSummary[]> {
  const all: StructureSummary[] = [];
  let cursor: string | null | undefined;
  do {
    const path = cursor === undefined || cursor === null ? "/v1/structures" : `/v1/structures?cursor=${encodeURIComponent(cursor)}`;
    const page = await readJson(path, Page, { ...options, release });
    all.push(...page.structures);
    cursor = page.next;
  } while (cursor !== undefined && cursor !== null);
  return all;
}

/** The record of structure `id` in `release`. */
export function structureOf(id: string, release: number, options: ReadOptions = {}): Promise<StructureDetail> {
  return readJson(`/v1/structures/${encodeURIComponent(id)}`, StructureDetail, { ...options, release });
}
