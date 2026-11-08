// What the object's record panel shows (read-an-object), worked out from the record alone: the
// sentence of what it is, its distances on one axis, its brightness and colour, its motion or
// velocity, and how many catalogues hold it. The panel draws these; nothing here is drawn.

import type { Measurement, ObjectRecord } from "../../api/objects.ts";
import { C_KM_S } from "../../scene/cosmology.ts";
import { PC_UNITS, figures, lightYears, shortDistance } from "../../scene/format.ts";
import { toMpc } from "../../scene/units.ts";
import type { DotRow } from "./charts.tsx";
import { colourIndices, publishedTypes } from "../starText.ts";
import { bandsText, words } from "../text.ts";

/** A source's id named as the panel names it. */
export type Namer = (id: string) => string;

/** What an object of each source is, where the source holds one kind of object. */
const NOUNS: Readonly<Record<string, string>> = {
  "erass1-clusters": "galaxy cluster or group",
  "cosmicflows-4-galaxies": "galaxy",
  "updated-nearby-galaxies": "galaxy",
  "mcconnachie-2012": "galaxy",
  twomrs: "galaxy",
  "twompp-catalogue": "galaxy",
  twompz: "galaxy",
  "wise-supercosmos": "galaxy",
  "sixdfgs-dr3": "galaxy",
  twodfgrs: "galaxy",
  "alfalfa-a100": "galaxy",
  "gaia-dr3-bright": "star",
  "hipparcos-2007": "star",
  gcns: "star",
};

/** The object's members, where it is a group. */
const membersOf = (record: ObjectRecord) => (record.relations ?? []).filter((r) => r.kind === "member" || r.kind === "dominant_member");

/** What the object is, as far as its record tells: a group by its kind, a star by its type, and
 * otherwise what its first source holds. */
function nounOf(record: ObjectRecord): string {
  if (record.kind === "galaxy_group") {
    const n = membersOf(record).length;
    return n === 0 ? "galaxy group" : `group of ${n.toLocaleString("en-GB")} galaxies`;
  }
  const type = publishedTypes(record).find((t) => /^[OBAFGKM]/i.test(t.text));
  if (type !== undefined) return `${type.text.charAt(0).toUpperCase()}-type star`;
  return NOUNS[record.entries[0]?.source ?? ""] ?? "object";
}

/** `noun` with its article: "an" before a vowel sound, a letter such as F included. */
const withArticle = (noun: string): string => `${/^([aeiou]|[AEFHILMNORSX]-)/i.test(noun) ? "an" : "a"} ${noun}`;

/** How far away the preferred distance puts the object, and by what where it is a redshift. */
function distancePart(record: ObjectRecord): string {
  const preferred = record.preferred;
  if (preferred === null) return ", with a sky position only, so not placed";
  const rank = preferred.rank ?? "";
  const by = rank.includes("photometric") ? ", by its photometric redshift" : rank.includes("redshift") ? ", by its spectroscopic redshift" : "";
  return ` ${shortDistance(preferred.distance_mpc)} away${by}`;
}

/** One sentence of what the object is, how far away, and in which scale domain. */
export function objectLede(record: ObjectRecord): string {
  const brightest = (record.relations ?? []).some((r) => r.kind === "dominant_member_of") ? ", the brightest of its group" : "";
  const domain = record.position === null ? "" : `, in the ${words(record.position.scale_domain)} domain`;
  const text = `${withArticle(nounOf(record))}${distancePart(record)}${brightest}${domain}.`;
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A published uncertainty as how far below and above its value it runs, where it says. */
function spread(u: Measurement["native"]["uncertainty"]): [number, number] | undefined {
  if (u?.form === "symmetric") return [u.sigma ?? 0, u.sigma ?? 0];
  if (u?.minus !== undefined && u.plus !== undefined) return [u.minus, u.plus];
  return undefined;
}

/** A distance or parallax measurement as a distance in megaparsecs with its range; a parallax at
 * its inverse. None where it cannot be told in megaparsecs. */
function distanceMpc(m: Measurement): { value: number; low?: number; high?: number } | undefined {
  const s = spread(m.native.uncertainty);
  if (m.quantity === "parallax") {
    const mas = m.native.value;
    if (!(mas > 0)) return undefined;
    const at = (p: number) => 1e-3 / p;
    return s === undefined || mas - s[0] <= 0 ? { value: at(mas) } : { value: at(mas), low: at(mas + s[1]), high: at(mas - s[0]) };
  }
  const k = toMpc(m.native.unit);
  if (k !== undefined) return s === undefined ? { value: m.native.value * k } : { value: m.native.value * k, low: (m.native.value - s[0]) * k, high: (m.native.value + s[1]) * k };
  const n = m.normalized;
  if (n === null || n === undefined || n.unit !== "megaparsec") return undefined;
  const ns = spread(n.uncertainty);
  return ns === undefined ? { value: n.value } : { value: n.value, low: n.value - ns[0], high: n.value + ns[1] };
}

/** The parsec unit that suits `mpc`: its size in parsecs and its name. */
export function unitFor(mpc: number): { size: number; name: string } {
  const [size, name] = PC_UNITS.find(([u]) => mpc * 1e6 >= u) ?? PC_UNITS[PC_UNITS.length - 1] ?? [1, "pc"];
  return { size, name };
}

const isPreferred = (record: ObjectRecord, m: Measurement): boolean => {
  const key = record.preferred?.measurement;
  return key !== undefined && key.entry === m.entry && key.quantity === m.quantity && key.n === m.n;
};

/** The object's distance: the preferred one large, every distance and parallax on one axis in
 * the unit that suits it, and what preferred it. */
export interface DistanceView {
  figure: string;
  unit: string;
  minus: string | undefined;
  plus: string | undefined;
  aside: string;
  kind: string | undefined;
  rows: DotRow[];
  parallaxes: { text: string; kind: string }[];
  rule: string;
}

/** A measurement's value in `unit`, to four figures, and its uncertainty, to two. */
const inUnit = (mpc: number, unit: { size: number }) => figures((mpc * 1e6) / unit.size, 4);
const spreadIn = (mpc: number, unit: { size: number }) => figures((mpc * 1e6) / unit.size, 2);

/** The distance part of the panel, where the object has a preferred distance. */
export function distanceView(record: ObjectRecord, name: Namer): DistanceView | undefined {
  const preferred = record.preferred;
  if (preferred === null) return undefined;
  const unit = unitFor(preferred.distance_mpc);
  const chosen = record.measurements.find((m) => isPreferred(record, m));
  const measured = record.measurements.filter((m) => m.quantity === "distance" || m.quantity === "parallax");
  const rows = measured.flatMap((m) => dotRow(record, m, unit, (id) => labelOf(measured, m, name(id))));
  const range = chosen === undefined ? undefined : distanceMpc(chosen);
  const redshift = chosen?.quantity === "redshift" ? redshiftOf(chosen) : undefined;
  return {
    figure: inUnit(preferred.distance_mpc, unit),
    unit: unit.name,
    minus: range?.low === undefined ? undefined : spreadIn(preferred.distance_mpc - range.low, unit),
    plus: range?.high === undefined ? undefined : spreadIn(range.high - preferred.distance_mpc, unit),
    aside: redshift === undefined ? lightYears(preferred.distance_mpc) : `from z = ${redshift}`,
    kind: chosen?.kind_of_value,
    rows: rows.length > 1 ? rows : [],
    parallaxes: measured.filter((m) => m.quantity === "parallax").map((m) => ({ text: `${name(m.source)} ${shortValue(m.native)} mas`, kind: m.kind_of_value })),
    rule: `Preferred: ${chosen === undefined ? "its measurement" : name(chosen.source)}, by the rule “${words(preferred.rule)}”${preferred.decided === undefined ? "" : `, decided by ${words(preferred.decided)}`}.`,
  };
}

/** A distance's source, with its kind where the source gives more than one distance. */
function labelOf(measured: readonly Measurement[], m: Measurement, source: string): string {
  const kind = m.distance?.kind;
  const several = measured.filter((o) => o.source === m.source && o.quantity === m.quantity).length > 1;
  return several && kind !== undefined ? `${source}, ${words(kind)}` : source;
}

function dotRow(record: ObjectRecord, m: Measurement, unit: { size: number; name: string }, name: Namer): DotRow[] {
  const d = distanceMpc(m);
  if (d === undefined) return [];
  const scale = (mpc: number) => (mpc * 1e6) / unit.size;
  return [
    {
      label: `${name(m.source)}${m.quantity === "parallax" ? ", 1/ϖ" : ""}`,
      value: scale(d.value),
      low: d.low === undefined ? undefined : scale(d.low),
      high: d.high === undefined ? undefined : scale(d.high),
      preferred: isPreferred(record, m),
      text: `${inUnit(d.value, unit)} ${unit.name}${m.quantity === "parallax" ? ", the inverse of its parallax" : ""}, ${m.kind_of_value}`,
    },
  ];
}

/** A number's text with a true minus sign. */
const minus = (text: string): string => text.replace(/^-/, "−");

/** A value to four figures with its symmetric uncertainty where it has one. */
function shortValue(value: Measurement["native"]): string {
  const s = spread(value.uncertainty);
  const v = minus(figures(value.value, 4));
  if (s === undefined) return v;
  return s[0] === s[1] ? `${v} ± ${figures(s[0], 2)}` : `${v} +${figures(s[1], 2)} −${figures(s[0], 2)}`;
}

/** A redshift measurement as z, from z or from cz. */
function redshiftOf(m: Measurement): string {
  const z = m.native.unit === "kilometre_per_second" ? m.native.value / C_KM_S : m.native.value;
  const s = spread(m.native.uncertainty);
  const sigma = s === undefined || m.native.unit === "kilometre_per_second" ? "" : ` ± ${figures(s[0], 2)}`;
  return `${minus(figures(z, 4))}${sigma}`;
}

/** A line of the panel as data: what it is, the value, its source's short name, its kind. */
export interface LineData {
  label: string;
  value: string;
  source: string;
  kind: string;
}

/** How each band of a magnitude is labelled. */
const BANDS: Readonly<Record<string, string>> = { phot_g_mean_mag: "G", Gmag: "G", Hpmag: "Hp", Bmag: "B", Vmag: "V", Ksmag: "Ks", Kcmag: "Kc", Jmag: "J", Hmag: "H", Kmag: "K" };

const bandLabel = (band: string): string => BANDS[band] ?? band.replace(/^phot_/, "").replace(/_?(mean_)?mag$/i, "");

/** The first measurement of each label, so that one line stands for each band. */
function firstOf(lines: LineData[]): LineData[] {
  const seen = new Set<string>();
  return lines.filter((l) => !seen.has(l.label) && seen.add(l.label));
}

/** The magnitudes, colour indices and spectral types of the object, one line for each. */
export function brightnessLines(record: ObjectRecord, name: Namer): LineData[] {
  const magnitudes = record.measurements
    .filter((m) => m.quantity === "magnitude")
    .map((m) => ({ label: bandLabel(m.band ?? "mag"), value: shortValue(m.native), source: name(m.source), kind: m.kind_of_value }));
  const colours = colourIndices(record).map((m) => ({ label: bandsText(m.band ?? "colour"), value: shortValue(m.native), source: name(m.source), kind: m.kind_of_value }));
  const types = publishedTypes(record).map((t) => ({ label: "Type", value: t.text, source: name(t.source), kind: t.kind }));
  return [...firstOf(magnitudes), ...firstOf(colours), ...types];
}

/** The object's BP−RP colour index, where one source gives one. */
export const bpRpOf = (record: ObjectRecord): number | undefined => colourIndices(record).find((m) => m.band === "BP-RP")?.native.value;

/** The proper motion as a total and its position angle, from the first source that gives both
 * components, and the first radial velocity. */
export function motionLines(record: ObjectRecord, name: Namer): LineData[] {
  const lines: LineData[] = [];
  const ra = record.measurements.find((m) => m.quantity === "proper_motion_ra" && record.measurements.some((d) => d.entry === m.entry && d.quantity === "proper_motion_dec"));
  const dec = ra === undefined ? undefined : record.measurements.find((m) => m.entry === ra.entry && m.quantity === "proper_motion_dec");
  if (ra !== undefined && dec !== undefined) {
    const total = Math.hypot(ra.native.value, dec.native.value);
    const angle = ((Math.atan2(ra.native.value, dec.native.value) * 180) / Math.PI + 360) % 360;
    lines.push({ label: "Proper", value: `${figures(total, 4)} mas/yr to PA ${Math.round(angle)}°`, source: name(ra.source), kind: ra.kind_of_value });
  }
  const radial = record.measurements.find((m) => m.quantity === "velocity");
  if (radial !== undefined) lines.push({ label: "Radial", value: `${shortValue(radial.native)} km/s`, source: name(radial.source), kind: radial.kind_of_value });
  return lines;
}

/** A redshift or velocity measurement as cz, km/s. */
const czOf = (m: Measurement): number => (m.native.unit === "kilometre_per_second" ? m.native.value : m.native.value * C_KM_S);

/** What a redshift's line is labelled by: its frame, or that it is photometric. */
function frameLabel(m: Measurement): string {
  if (m.quantity === "velocity") return "Velocity";
  if (m.redshift?.kind === "photometric") return "Photometric z";
  const frame = m.redshift?.published_frame;
  return frame === "cmb" ? "CMB frame" : frame === undefined ? "Redshift" : frame.charAt(0).toUpperCase() + frame.slice(1);
}

/** A redshift or velocity measurement in words: cz in km/s, or z where it was published as z. */
const velocityText = (m: Measurement): string =>
  m.redshift?.kind === "photometric" || (m.native.unit !== "kilometre_per_second" && Math.abs(m.native.value) > 0.05) ? `z = ${redshiftOf(m)}` : `${shortValue({ ...m.native, value: czOf(m) })} km/s`;

/** How many redshifts one source gives before they are drawn as ticks rather than lines. */
const STRIP = 3;

/** The object's velocity: a line for each frame, and the source that gives many redshifts as
 * ticks, marked with another source's velocity. */
export interface VelocityView {
  lines: LineData[];
  strip: { values: number[]; source: string; mark: number | undefined } | undefined;
}

export function velocityView(record: ObjectRecord, name: Namer): VelocityView {
  const redshifts = new Set(record.measurements.filter((m) => m.quantity === "redshift").map((m) => m.entry));
  // A source's velocity repeats the redshift of its entry where it gives one.
  const all = record.measurements.filter((m) => m.quantity === "redshift" || (m.quantity === "velocity" && !redshifts.has(m.entry)));
  const counts = new Map<string, number>();
  for (const m of all.filter((m) => m.quantity === "redshift")) counts.set(m.source, (counts.get(m.source) ?? 0) + 1);
  const many = [...counts].find(([, n]) => n > STRIP)?.[0];
  const rest = all.filter((m) => m.source !== many);
  const lines = rest.map((m) => ({ label: frameLabel(m), value: velocityText(m), source: name(m.source), kind: m.kind_of_value }));
  const mark = rest.find((m) => m.redshift?.published_frame === "heliocentric" || m.quantity === "velocity");
  return {
    lines: firstOf(lines),
    strip: many === undefined ? undefined : { values: all.filter((m) => m.source === many).map(czOf), source: name(many), mark: mark === undefined ? undefined : czOf(mark) },
  };
}

/** How many catalogue entries hold the object, in how many catalogues, and how many in each. */
export function cataloguesText(record: ObjectRecord, name: Namer): { title: string; detail: string } {
  const counts = new Map<string, number>();
  for (const e of record.entries) counts.set(e.source, (counts.get(e.source) ?? 0) + 1);
  const n = record.entries.length;
  const plural = (k: number, word: string) => `${k.toLocaleString("en-GB")} ${word}${k === 1 ? "" : "s"}`;
  const title = n === counts.size ? `In ${plural(n, "catalogue")}` : `In ${plural(n, "entry")} of ${plural(counts.size, "catalogue")}`.replace("entrys", "entries");
  const detail = [...counts]
    .sort((a, b) => b[1] - a[1])
    .map(([source, k]) => (k === 1 ? name(source) : `${name(source)} ${k.toLocaleString("en-GB")}`))
    .join(", ");
  return { title, detail: `${detail}.` };
}
