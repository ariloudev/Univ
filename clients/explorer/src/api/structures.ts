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

/** Where in its paper a value is printed, and the words. */
const Cite = z.looseObject({ paper: z.string(), locator: z.looseObject({ quote: z.string().optional() }).nullable().optional() });

const Cited = z.looseObject({
  kind: z.string().optional(),
  quantity: z.looseObject({ value: z.unknown(), unit: z.string().optional(), uncertainty: z.unknown().optional() }).optional(),
  cite: Cite.nullable().optional(),
});
export type Cited = z.infer<typeof Cited>;

const Paper = z.looseObject({ id: z.string(), reference: z.string(), title: z.string().optional() });

/** A bound in depth: a velocity cz, or a distance. */
const Depth = z.looseObject({ velocity: Cited.optional(), distance: Cited.optional() });

/** The record's geometry as published, as far as the panel reads it: a box on the sky between two
 * depths, a primitive about a centre, or members. */
const RecordGeometry = z.looseObject({
  SkyRegion: z
    .looseObject({
      frame: z.string(),
      /** Its longitude and latitude ranges, or its centre and its size along each. */
      lon: z.array(Cited).optional(),
      lat: z.array(Cited).optional(),
      centre: z.array(Cited).optional(),
      size: z.array(Cited).optional(),
      near: Depth.nullable().optional(),
      far: Depth.nullable().optional(),
    })
    .optional(),
  Primitive: z.looseObject({ centre: z.looseObject({ form: z.string() }).optional(), size: z.record(z.string(), Cited).optional() }).optional(),
});

/** Its published position: on the sky, with its depth where published, or as Cartesian velocities. */
const RecordPosition = z.looseObject({
  frame: z.string(),
  lon: Cited.nullable().optional(),
  lat: Cited.nullable().optional(),
  radial: Depth.nullable().optional(),
  x: Cited.optional(),
  y: Cited.optional(),
  z: Cited.optional(),
});

/** A value whose paper assumed another Hubble constant, and that value in the release cosmology. */
const Rescaled = z.looseObject({
  value: z.string(),
  native: z.looseObject({ value: z.number(), hubble_constant: z.number() }),
  rescaled: z.looseObject({ value: z.number() }),
});

const StructureDetail = z.looseObject({
  id: z.string(),
  name: z.string(),
  kind: z.string(),
  status: z.string().optional(),
  scale_domain: z.string(),
  centre: Centre.nullable(),
  bounds: z.looseObject({ min: Triple, max: Triple }).nullable(),
  shape: Shape.nullable(),
  derivation: z
    .looseObject({
      from: z.array(z.string()).optional(),
      /** Where its papers give the shape as a lower limit on its extent: a pointer into the record. */
      at_least: z.string().optional(),
      /** Its published extents about its point, where its papers give no bounds: each axis of
       * their frame as a whole-extent vector in the ICRS, in the shape's unit. */
      extents: z.looseObject({ frame: z.string(), axes: z.tuple([Triple, Triple, Triple]) }).optional(),
      /** How its distance was placed, in Univ Server's words: from what, and by what method. */
      distance: z.looseObject({ from: z.string(), method: z.string() }).optional(),
    })
    .nullable()
    .optional(),
  /** Its record as researched, which only its panel reads: see `readPanelRecord`. */
  record: z.looseObject({}),
});
export type StructureDetail = z.infer<typeof StructureDetail>;

/** What a structure's panel reads besides what the view draws: its record as researched, its
 * values rescaled to the release cosmology, and its members' distances. */
const PanelRecord = z.looseObject({
  record: z.looseObject({
    alternative_names: z.array(z.string()).nullable().optional(),
    notes: z.string().nullable().optional(),
    disputes: z.array(z.looseObject({ cite: Cite, claim: z.string().optional() })).nullable().optional(),
    geometry: RecordGeometry.nullable().optional(),
    position: RecordPosition.nullable().optional(),
    papers: z.array(Paper).optional(),
    values: z.record(z.string(), Cited).optional(),
  }),
  rescaled: z.array(Rescaled).nullable().optional(),
  members: z.looseObject({ placed_from_file: z.array(z.looseObject({ distance_mpc: z.number() })).optional() }).nullable().optional(),
});

/** A structure as its panel reads it. */
export type StructureRecord = StructureDetail & z.infer<typeof PanelRecord>;

/** The structure `detail` as its panel reads it, or what in its record could not be read. The view
 * draws a structure whatever its record holds; its panel says when it cannot read it. */
export function readPanelRecord(detail: StructureDetail): { structure: StructureRecord } | { problem: string } {
  const read = PanelRecord.safeParse(detail);
  if (read.success) return { structure: { ...detail, ...read.data } };
  const issue = read.error.issues[0];
  return { problem: issue === undefined ? "its record is not as expected" : `${issue.path.join(".")}: ${issue.message}` };
}

/** A structure as Univ Server gives it, read and checked as its panel reads it. */
export function readStructure(raw: unknown): StructureRecord {
  const read = readPanelRecord(StructureDetail.parse(raw));
  if ("problem" in read) throw new Error(read.problem);
  return read.structure;
}

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
