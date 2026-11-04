// Univ Server's objects as the explorer reads them (read-an-object, find-an-object), checked as
// they arrive. Members the explorer does not read are let through unchecked.

import { z } from "zod";

import { type ReadOptions, read } from "./client.ts";

const Uncertainty = z.looseObject({
  form: z.string(),
  sigma: z.number().optional(),
  minus: z.number().optional(),
  plus: z.number().optional(),
});

const Value = z.looseObject({ value: z.number(), unit: z.string(), uncertainty: Uncertainty.optional() });

const Measurement = z.looseObject({
  entry: z.number().int(),
  source: z.string(),
  quantity: z.string(),
  n: z.number().int(),
  kind_of_value: z.string(),
  native: Value,
  normalized: Value.nullable().optional(),
});
export type Measurement = z.infer<typeof Measurement>;

const MeasurementKey = z.looseObject({ entry: z.number().int(), quantity: z.string(), n: z.number().int() });

const Preferred = z.looseObject({
  rule: z.string(),
  measurement: MeasurementKey,
  distance_mpc: z.number(),
  /** Fractional: the uncertainty over the distance. */
  uncertainty: z.looseObject({ form: z.string(), total: z.number().nullable().optional() }),
});

const Position = z.looseObject({
  frame: z.string(),
  scale_domain: z.string(),
  unit: z.enum(["parsec", "megaparsec"]),
  x: z.number(),
  y: z.number(),
  z: z.number(),
  epoch: z.number(),
});

const SkyPosition = z.looseObject({ frame: z.string(), epoch: z.number().nullable().optional() });

const ObjectRecord = z.looseObject({
  release: z.number().int(),
  id: z.number().int(),
  kind: z.string(),
  entries: z.array(z.looseObject({ source: z.string(), identifier: z.string(), name: z.string().nullable() })),
  designations: z.array(z.looseObject({ source: z.string(), identifier: z.string() })),
  measurements: z.array(Measurement),
  preferred: Preferred.nullable(),
  position: Position.nullable(),
  sky_position: SkyPosition.nullable().optional(),
});
export type ObjectRecord = z.infer<typeof ObjectRecord>;

const ObjectChoices = z.looseObject({
  objects: z.array(z.looseObject({ id: z.number().int(), names: z.array(z.string()).optional() })),
});

/** What a search found. */
export type Found =
  | { kind: "object"; record: ObjectRecord }
  | { kind: "choices"; query: string; objects: { id: number; names: string[] }[] }
  | { kind: "none"; query: string };

/** The path that looks `query` up: an id for a number, a designation for `source:identifier`,
 * a name otherwise. */
export function lookupPath(query: string): string {
  const text = query.trim();
  if (/^\d+$/.test(text)) return `/v1/objects/${text}`;
  const designation = /^([a-z0-9][a-z0-9-]*):(.+)$/i.exec(text);
  if (designation !== null) {
    const [, source = "", identifier = ""] = designation;
    return `/v1/objects?source=${encodeURIComponent(source)}&designation=${encodeURIComponent(identifier.trim())}`;
  }
  return `/v1/objects?name=${encodeURIComponent(text)}`;
}

/** The object `id` of `release`. */
export async function objectOf(id: number, options: ReadOptions = {}): Promise<ObjectRecord> {
  const response = await read(`/v1/objects/${id}`, options);
  return ObjectRecord.parse(await response.json());
}

/** Looks `query` up in `options.release`: the object, the objects to choose from, or none. */
export async function search(query: string, options: ReadOptions = {}): Promise<Found> {
  const response = await read(lookupPath(query), { ...options, accept: [300, 404] });
  if (response.status === 404) return { kind: "none", query };
  const body: unknown = await response.json();
  if (response.status === 300) {
    const objects = ObjectChoices.parse(body).objects.map((o) => ({ id: o.id, names: o.names ?? [] }));
    return { kind: "choices", query, objects };
  }
  return { kind: "object", record: ObjectRecord.parse(body) };
}

/** The megaparsecs in each unit an object's position may be in. */
const MPC_PER = { parsec: 1e-6, megaparsec: 1 } as const;

/** The object's ICRS position in megaparsecs; none for an object with no distance. */
export function placeOf(record: ObjectRecord): [number, number, number] | undefined {
  const position = record.position;
  if (position === null) return undefined;
  const k = MPC_PER[position.unit];
  return [position.x * k, position.y * k, position.z * k];
}

const sameKey = (m: Measurement, key: z.infer<typeof MeasurementKey>) =>
  m.entry === key.entry && m.quantity === key.quantity && m.n === key.n;

/** The measurement the preferred distance was taken from, and every other distance. */
export function distancesOf(record: ObjectRecord): { preferred: Measurement | undefined; others: Measurement[] } {
  const key = record.preferred?.measurement;
  const preferred = key === undefined ? undefined : record.measurements.find((m) => sameKey(m, key));
  const others = record.measurements.filter(
    (m) => m !== preferred && (m.quantity === "distance" || m.quantity === "parallax"),
  );
  return { preferred, others };
}
