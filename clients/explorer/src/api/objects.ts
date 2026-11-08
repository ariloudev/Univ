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
  /** The band of a magnitude, or the bands of a colour index such as `B-V`. */
  band: z.string().nullable().optional(),
  /** The two magnitudes a colour index was computed from, where its source publishes no colour. */
  derived_from: z.array(z.string()).nullable().optional(),
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

/** The object's preferred sky position: ICRS degrees at its epoch, from one entry. */
const SkyPosition = z.looseObject({
  frame: z.string(),
  ra: z.number().optional(),
  dec: z.number().optional(),
  epoch: z.number().nullable().optional(),
  entry: z.number().int().optional(),
  uncertainty_mas: z.number().nullable().optional(),
});

/** A source and the identifier it gives. */
const SourceIdentifier = z.looseObject({ source: z.string(), identifier: z.string() });

/** How one object stands to another: a group to its dominant galaxy or a member, and back. */
const Relation = z.looseObject({ kind: z.string(), entry: SourceIdentifier, other: SourceIdentifier, scheme: z.string(), identifier: z.string() });
export type Relation = z.infer<typeof Relation>;

const ObjectRecord = z.looseObject({
  release: z.number().int(),
  id: z.number().int(),
  kind: z.string(),
  entries: z.array(
    z.looseObject({
      source: z.string(),
      identifier: z.string(),
      /** Its identifier in its catalogue's form, where its source's definition gives one. */
      designation: z.string().nullable().optional(),
      name: z.string().nullable(),
      /** Its source's quality columns, by column, as published. */
      flags: z.record(z.string(), z.unknown()).optional(),
      spectral_type: z.looseObject({ text: z.string(), kind_of_value: z.string() }).nullable().optional(),
      references: z
        .array(z.looseObject({ citation: z.string(), doi: z.string().nullable().optional(), bibcode: z.string().nullable().optional() }))
        .optional(),
    }),
  ),
  designations: z.array(SourceIdentifier.extend({ designation: z.string().nullable().optional() })),
  /** Every common or proper name its sources give it, with the source. */
  names: z.array(z.looseObject({ name: z.string(), source: z.string() })).optional(),
  relations: z.array(Relation).optional(),
  not_placed: z.string().nullable().optional(),
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
export type Found = { kind: "object"; record: ObjectRecord; as?: string | undefined } | Choices | { kind: "none"; query: string };

/** The problem type of a lookup that found no object. */
const NOT_FOUND = "urn:univ:problem:object-not-found";

/** What `query` looks up: an id for a number, a designation for `source:identifier`, a name
 * otherwise. */
function lookupOf(query: string): { by: "id" | "designation" | "name"; path: string } {
  const text = query.trim();
  if (/^\d+$/.test(text)) return { by: "id", path: `/v1/objects/${text}` };
  const designation = /^([a-z0-9][a-z0-9-]*):(.+)$/i.exec(text);
  if (designation !== null) {
    const [, source = "", identifier = ""] = designation;
    return { by: "designation", path: `/v1/objects?source=${encodeURIComponent(source)}&designation=${encodeURIComponent(identifier.trim())}` };
  }
  return { by: "name", path: `/v1/objects?name=${encodeURIComponent(text)}` };
}

/** The path that looks `query` up. */
export const lookupPath = (query: string): string => lookupOf(query).path;

/** A name as the name lookup compares it: letter case and white space ignored. */
const nameKey = (text: string) => text.toLowerCase().replace(/\s+/g, "");

/** The name or designation of `record` that reads as `text`, letter case and white space aside,
 * as `record` spells it; none where it has none. */
export function nameOf(record: ObjectRecord, text: string): string | undefined {
  const key = nameKey(text);
  const spellings = [...record.entries.flatMap((e) => (e.name === null ? [] : [e.name])), ...record.designations.map((d) => d.identifier)];
  return spellings.find((s) => nameKey(s) === key);
}

/** The name `record` was found by when looked up by the name `query`: its name or designation
 * that reads as the query, or the query itself (Univ Server also matches names the record does
 * not carry). */
const nameFound = (record: ObjectRecord, query: string): string => nameOf(record, query) ?? query.trim();

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

/** Looks `query` up in `options.release`: the object, with the name it was found by where it was
 * looked up by name; the objects to choose from; or none. */
export async function search(query: string, options: ReadOptions = {}): Promise<Found> {
  const { by, path } = lookupOf(query);
  try {
    const found = await lookup(path, query, options);
    return found.kind === "object" && by === "name" ? { ...found, as: nameFound(found.record, query) } : found;
  } catch (error) {
    if (error instanceof ApiError && error.type === NOT_FOUND) return { kind: "none", query };
    throw error;
  }
}

/** The records read most recently, by release and id: the hover label and the panel share them,
 * so a point rested on and then chosen is read once. One that fails is read again next time. */
const RECENT = 64;
const recent = new Map<string, Promise<Exclude<Found, { kind: "none" }>>>();

/** Object `id` of `release`, read once while it is among the `RECENT` read last. */
export function recordOf(id: number, release: number): Promise<Exclude<Found, { kind: "none" }>> {
  const key = `${release}:${id}`;
  const found = recent.get(key) ?? objectOf(id, { release });
  recent.delete(key);
  recent.set(key, found);
  found.catch(() => recent.delete(key));
  const oldest = recent.keys().next().value;
  if (recent.size > RECENT && oldest !== undefined) recent.delete(oldest);
  return found;
}

/** Forgets every record read, as a test begins. */
export const forgetRecords = (): void => {
  recent.clear();
};

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
