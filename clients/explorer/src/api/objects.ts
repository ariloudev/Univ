// Univ Server's objects as the explorer reads them (read-an-object, find-an-object), checked as
// they arrive. Members the explorer does not read are let through unchecked.

import { z } from "zod";

import { toMpc } from "../scene/units.ts";
import { ApiError, type ReadOptions, read } from "./client.ts";

const Uncertainty = z.looseObject({
  form: z.string(),
  sigma: z.number().optional(),
  minus: z.number().optional(),
  plus: z.number().optional(),
  probability: z.number().optional(),
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
  unit: z.string(),
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

/** The objects a lookup leads to, to choose from. */
export interface Choices {
  kind: "choices";
  query: string;
  objects: { id: number; names: string[] }[];
}

/** What a search found. */
export type Found = { kind: "object"; record: ObjectRecord } | Choices | { kind: "none"; query: string };

/** The problem type of a lookup that found no object. */
const NOT_FOUND = "urn:univ:problem:object-not-found";

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

/** A name as the name lookup compares it: letter case and white space ignored. */
const nameKey = (text: string) => text.toLowerCase().replace(/\s+/g, "");

/** The name `record` was found by when `query` looked it up by name: the designation that reads
 * as the query, or the query itself; none when `query` was an id or a designation. */
export function nameFound(record: ObjectRecord, query: string): string | undefined {
  if (!lookupPath(query).startsWith("/v1/objects?name=")) return undefined;
  const key = nameKey(query);
  return record.designations.find((d) => nameKey(d.identifier) === key)?.identifier ?? query.trim();
}

/** Reads `path`: the object, or, answered 300, the objects it leads to. */
async function lookup(path: string, query: string, options: ReadOptions): Promise<Exclude<Found, { kind: "none" }>> {
  const response = await read(path, { ...options, accept: [300] });
  const body: unknown = await response.json();
  if (response.status === 300) {
    const objects = ObjectChoices.parse(body).objects.map((o) => ({ id: o.id, names: o.names ?? [] }));
    return { kind: "choices", query, objects };
  }
  return { kind: "object", record: ObjectRecord.parse(body) };
}

/** The object `id` of `options.release`, or the objects that replaced it where it was split. */
export function objectOf(id: number, options: ReadOptions = {}): Promise<Exclude<Found, { kind: "none" }>> {
  return lookup(`/v1/objects/${id}`, String(id), options);
}

/** Looks `query` up in `options.release`: the object, the objects to choose from, or none. */
export async function search(query: string, options: ReadOptions = {}): Promise<Found> {
  try {
    return await lookup(lookupPath(query), query, options);
  } catch (error) {
    if (error instanceof ApiError && error.type === NOT_FOUND) return { kind: "none", query };
    throw error;
  }
}

/** The object's ICRS position in megaparsecs; none for an object with no distance. */
export function placeOf(record: ObjectRecord): [number, number, number] | undefined {
  const position = record.position;
  const k = position === null ? undefined : toMpc(position.unit);
  if (position === null || k === undefined) return undefined;
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
