// How the object's panel words its record (read-an-object): designations in their catalogue's
// form, merged where sources share one; the sky position in sexagesimal ICRS and in galactic
// longitude and latitude; the object's groups; and each flag with the meaning its source gives.

import type { ObjectRecord, Relation } from "../api/objects.ts";
import type { SourceDetail } from "../api/sources.ts";
import { figures } from "../scene/format.ts";
import { lonLat } from "../scene/graticule.ts";
import { type Mat3, apply } from "../scene/vec.ts";

/** A name as it is compared: letter case and white space aside, as Univ Server resolves one. */
const folded = (text: string): string => text.toLowerCase().replace(/\s+/g, "");

/** The object's proper names, each written once with every source that gives it, and then its
 * designations as `mergedDesignations` gives them, less those that read as one of its names,
 * whose sources the name takes: a catalogue whose identifier is a name, such as the IAU's
 * "Sirius" or OpenNGC's "M 31", gives it once. */
export function namesAndDesignations(record: ObjectRecord): { names: Listed[]; designations: Listed[] } {
  const names = new Map<string, Listed>();
  for (const n of record.names ?? []) {
    const found = names.get(folded(n.name)) ?? { text: n.name, sources: [] };
    if (!found.sources.includes(n.source)) found.sources.push(n.source);
    names.set(folded(n.name), found);
  }
  const designations = mergedDesignations(record).filter((d) => {
    const name = names.get(folded(d.text));
    if (name === undefined) return true;
    for (const source of d.sources) if (!name.sources.includes(source)) name.sources.push(source);
    return false;
  });
  return { names: [...names.values()], designations };
}

/** A name or designation listed once, with every source that gives it. */
interface Listed {
  text: string;
  sources: string[];
}

/** `text` with its sign: a minus for a negative value and a plus otherwise. */
const signed = (negative: boolean, text: string): string => `${negative ? "−" : "+"}${text}`;

type Entry = ObjectRecord["entries"][number];

/** A designation as its catalogue writes it, or as its source gives it where no form is known. */
export const designationText = (d: { designation?: string | null | undefined; identifier: string }): string => d.designation ?? d.identifier;

/** The object's designations, each written once with every source that gives it, in the
 * record's order. Only a designation in its catalogue's form is merged across sources: a bare
 * identifier is one source's own number, which another catalogue may give another object. */
export function mergedDesignations(record: ObjectRecord): Listed[] {
  const merged = new Map<string, Listed>();
  for (const d of record.designations) {
    const key = d.designation ?? `${d.source}\u0000${d.identifier}`;
    const found = merged.get(key) ?? { text: designationText(d), sources: [] };
    if (!found.sources.includes(d.source)) found.sources.push(d.source);
    merged.set(key, found);
  }
  return [...merged.values()];
}

/** A galactic longitude and latitude, degrees to four decimals, the longitude 0 to 360 after its
 * rounding. */
export function galacticText({ l, b }: { l: number; b: number }): string {
  const lon = (Math.round(l * 1e4) / 1e4) % 360;
  return `l ${lon.toFixed(4)}°, b ${signed(b < 0, `${Math.abs(b).toFixed(4)}°`)}`;
}

/** A sky position's published error in milliarcseconds, to two figures. */
export const masText = (mas: number | null | undefined): string => (mas === null || mas === undefined ? "no uncertainty reported" : `± ${figures(mas, 2)} mas`);

/** The designation `identifier` in the form its source's catalogue writes it, where the record of
 * that source gives one. */
export const formedAs = (identifier: string, detail: SourceDetail | undefined): string =>
  detail?.designation?.form.replace("{identifier}", identifier) ?? identifier;

/** `value` split into whole units, sixtieths and the sixtieths of those, the last to `places`
 * decimals, carried where it rounds up to 60. */
function sexagesimal(value: number, places: number): [number, number, number] {
  const scale = 10 ** places;
  const total = Math.round(value * 3600 * scale);
  const whole = Math.floor(total / (3600 * scale));
  const minutes = Math.floor((total - whole * 3600 * scale) / (60 * scale));
  const seconds = (total - whole * 3600 * scale - minutes * 60 * scale) / scale;
  return [whole, minutes, seconds];
}

const two = (n: number): string => String(n).padStart(2, "0");
const secondsText = (s: number, places: number): string => s.toFixed(places).padStart(places + 3, "0");

/** Right ascension `deg` degrees in hours, minutes and seconds, to a hundredth of a second. */
export function raText(deg: number): string {
  const [h, m, s] = sexagesimal((((deg % 360) + 360) % 360) / 15, 2);
  return `${two(h % 24)}h ${two(m)}m ${secondsText(s, 2)}s`;
}

/** Declination `deg` degrees in degrees, arcminutes and arcseconds, to a tenth of an arcsecond,
 * with its sign. */
export function decText(deg: number): string {
  const [d, m, s] = sexagesimal(Math.abs(deg), 1);
  return signed(deg < 0, `${two(d)}° ${two(m)}′ ${secondsText(s, 1)}″`);
}

/** The galactic longitude and latitude, degrees, of ICRS `ra` and `dec`, through the rotation
 * `toGalactic` the release's manifests give. */
export function galacticOf(ra: number, dec: number, toGalactic: Mat3): { l: number; b: number } {
  const r = (ra * Math.PI) / 180;
  const d = (dec * Math.PI) / 180;
  const { lon, lat } = lonLat(apply(toGalactic, [Math.cos(d) * Math.cos(r), Math.cos(d) * Math.sin(r), Math.sin(d)]));
  return { l: lon, b: lat };
}

/** Each object the record is related to, by the designation its group's source gives it, with
 * every way it stands to it. */
export function relationsByOther(record: ObjectRecord): { other: Relation["other"]; kinds: string[] }[] {
  const byOther = new Map<string, { other: Relation["other"]; kinds: string[] }>();
  for (const r of record.relations ?? []) {
    const key = `${r.other.source}\u0000${r.other.identifier}`;
    const found = byOther.get(key) ?? { other: r.other, kinds: [] };
    if (!found.kinds.includes(r.kind)) found.kinds.push(r.kind);
    byOther.set(key, found);
  }
  return [...byOther.values()];
}

/** How the object stands to another in words, for `kinds` of relation. */
export function relationText(kinds: readonly string[]): string {
  if (kinds.includes("dominant_member_of")) return "The dominant galaxy of the group";
  if (kinds.includes("member_of")) return "A member of the group";
  if (kinds.includes("dominant_member")) return "The group's dominant galaxy";
  return "A member of this group";
}

/** Each flag of `entry` with its value as published and the meaning its source gives that
 * value, where it gives one. */
export function flagsOf(entry: Entry, detail: SourceDetail | undefined): { column: string; value: string; meaning: string | undefined; cite: string | undefined }[] {
  return Object.entries(entry.flags ?? {}).map(([column, raw]) => {
    const value = typeof raw === "string" ? raw : JSON.stringify(raw);
    const meanings = detail?.flags?.[column];
    return { column, value, meaning: meanings?.values[value], cite: meanings?.cite };
  });
}
