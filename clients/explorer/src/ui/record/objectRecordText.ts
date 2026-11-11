// What the object's record panel shows (read-an-object), worked out from the record alone: the
// sentence of what it is, its distances on one axis, its brightness and colour, its motion or
// velocity, and how many catalogues hold it. The panel draws these; nothing here is drawn.

import { type Measurement, type ObjectRecord, sameKey } from "../../api/objects.ts";
import { C_KM_S } from "../../scene/cosmology.ts";
import { figures, lightYears, pcUnit, shortDistance } from "../../scene/format.ts";
import type { StarColourIndex } from "../../scene/starColour.ts";
import { toMpc } from "../../scene/units.ts";
import { colourIndices, drawnColourIndex, publishedTypes } from "../starText.ts";
import { KIND_NOUNS, bandsText, words } from "../text.ts";
import type { DotRow } from "./charts.tsx";
import { counted, sentence, shortWithUncertainty, spread, withArticle } from "./wording.ts";

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
  // A tracer is what its source names it, before the type its Gaia star's entry may give.
  const kind = KIND_NOUNS[record.kind];
  if (kind !== undefined) return kind;
  const type = publishedTypes(record).find((t) => /^[OBAFGKM]/i.test(t.text));
  if (type !== undefined) return `${type.text.charAt(0).toUpperCase()}-type star`;
  return NOUNS[record.entries[0]?.source ?? ""] ?? "object";
}

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
  return sentence(`${withArticle(nounOf(record))}${distancePart(record)}${brightest}${domain}.`);
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

const isPreferred = (record: ObjectRecord, m: Measurement): boolean => {
  const key = record.preferred?.measurement;
  return key !== undefined && sameKey(m, key);
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
  const unit = pcUnit(preferred.distance_mpc);
  const chosen = record.measurements.find((m) => isPreferred(record, m));
  const measured = record.measurements.filter((m) => m.quantity === "distance" || m.quantity === "parallax");
  const several = countBy(measured, (m) => `${m.source}:${m.quantity}`);
  const rows = measured.flatMap((m) => dotRow(record, m, unit, (id) => labelOf(m, name(id), (several.get(`${m.source}:${m.quantity}`) ?? 0) > 1)));
  const range = chosen === undefined ? undefined : distanceMpc(chosen);
  const redshift = chosen?.quantity === "redshift" ? redshiftOf(chosen) : undefined;
  // Where the measurement gives no range in distance, as a redshift does not, the uncertainty
  // Univ Server gives the preferred distance as a fraction of it.
  const total = preferred.uncertainty.total;
  const fallback = total === null || total === undefined ? undefined : spreadIn(total * preferred.distance_mpc, unit);
  return {
    figure: inUnit(preferred.distance_mpc, unit),
    unit: unit.name,
    minus: range?.low === undefined ? fallback : spreadIn(preferred.distance_mpc - range.low, unit),
    plus: range?.high === undefined ? fallback : spreadIn(range.high - preferred.distance_mpc, unit),
    aside: redshift === undefined ? lightYears(preferred.distance_mpc) : `from z = ${redshift}`,
    kind: chosen?.kind_of_value,
    rows: rows.length > 1 ? rows : [],
    parallaxes: measured.filter((m) => m.quantity === "parallax").map((m) => ({ text: `${name(m.source)} ${shortValue(m.native)} mas`, kind: m.kind_of_value })),
    rule: `Preferred: ${chosen === undefined ? "its measurement" : name(chosen.source)}, by the rule “${words(preferred.rule)}”${preferred.decided === undefined ? "" : `, decided by ${words(preferred.decided)}`}.`,
  };
}

/** How many of `items` share each key. */
function countBy<T>(items: readonly T[], key: (item: T) => string): Map<string, number> {
  const counts = new Map<string, number>();
  for (const item of items) counts.set(key(item), (counts.get(key(item)) ?? 0) + 1);
  return counts;
}

/** A distance's source, with its kind where the source gives more than one distance. */
function labelOf(m: Measurement, source: string, several: boolean): string {
  const kind = m.distance?.kind;
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

/** A value to four figures with its uncertainty, in short, as its source published it. */
const shortValue = (value: Measurement["native"]): string => shortWithUncertainty(figures(value.value, 4), value.uncertainty);

const isCz = (m: Measurement): boolean => m.native.unit === "kilometre_per_second";

/** A redshift measurement as z with its uncertainty, from z or from cz. */
function redshiftOf(m: Measurement): string {
  const k = isCz(m) ? 1 / C_KM_S : 1;
  return shortWithUncertainty(figures(m.native.value * k, 4), m.native.uncertainty, k);
}

/** A redshift or velocity measurement as cz with its uncertainty, km/s. */
function czText(m: Measurement): string {
  const k = isCz(m) ? 1 : C_KM_S;
  return `${shortWithUncertainty(figures(m.native.value * k, 4), m.native.uncertainty, k)} km/s`;
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

/** The share of the posterior a credible interval holds, as a percentage, where it says one. */
const heldText = (m: Measurement): string => {
  const p = (m.normalized ?? m.native).uncertainty?.probability;
  return p === undefined ? "" : ` (${figures(p * 100, 2)}%)`;
};

/** The ages of the object (read-an-object): each in millions of years with its uncertainty, and
 * the base-10 logarithm of the age in years as its source publishes it, where it publishes one. */
export function ageLines(record: ObjectRecord, name: Namer): LineData[] {
  return record.measurements
    .filter((m) => m.quantity === "age")
    .flatMap((m) => {
      const years = m.normalized ?? m.native;
      const source = name(m.source);
      const kind = m.kind_of_value;
      const age = { label: "Age", value: `${shortWithUncertainty(figures(years.value / 1e6, 2), years.uncertainty, 1e-6)} Myr${heldText(m)}`, source, kind };
      if (m.native.unit !== "log_year") return [age];
      const published = shortWithUncertainty(m.native.value.toFixed(2), m.native.uncertainty);
      return [age, { label: "log age (yr)", value: `${published}, as published`, source, kind }];
    });
}

/** The colour index the view draws the star by, where it has one. */
export const drawnColourOf = (record: ObjectRecord): StarColourIndex | undefined => drawnColourIndex(colourIndices(record));

/** The first source's two components of proper motion, where a source gives both. */
function properMotionOf(record: ObjectRecord): { ra: Measurement; dec: Measurement } | undefined {
  for (const ra of record.measurements.filter((m) => m.quantity === "proper_motion_ra")) {
    const dec = record.measurements.find((m) => m.entry === ra.entry && m.quantity === "proper_motion_dec");
    if (dec !== undefined) return { ra, dec };
  }
  return undefined;
}

/** Whether a source gives the object's proper motion: a star's, which the panel shows as motion. */
export const hasProperMotion = (record: ObjectRecord): boolean => properMotionOf(record) !== undefined;

/** The proper motion as a total and its position angle, from the first source that gives both
 * components, and the first radial velocity. */
export function motionLines(record: ObjectRecord, name: Namer): LineData[] {
  const lines: LineData[] = [];
  const proper = properMotionOf(record);
  if (proper !== undefined) {
    const { ra, dec } = proper;
    const total = Math.hypot(ra.native.value, dec.native.value);
    const angle = ((Math.atan2(ra.native.value, dec.native.value) * 180) / Math.PI + 360) % 360;
    lines.push({ label: "Proper", value: `${figures(total, 4)} mas/yr to PA ${Math.round(angle)}°`, source: name(ra.source), kind: ra.kind_of_value });
  }
  const radial = record.measurements.find((m) => m.quantity === "velocity");
  if (radial !== undefined) lines.push({ label: "Radial", value: `${shortValue(radial.native)} km/s`, source: name(radial.source), kind: radial.kind_of_value });
  return lines;
}

/** A redshift or velocity measurement as cz, km/s. */
const czOf = (m: Measurement): number => (isCz(m) ? m.native.value : m.native.value * C_KM_S);

/** What a redshift's line is labelled by: its frame, or that it is photometric. */
function frameLabel(m: Measurement): string {
  if (m.quantity === "velocity") return "Velocity";
  if (m.redshift?.kind === "photometric") return "Photometric z";
  const frame = m.redshift?.published_frame;
  return frame === "cmb" ? "CMB frame" : frame === undefined ? "Redshift" : sentence(frame);
}

/** A redshift or velocity measurement in words: as z where it is photometric or far, else as cz in
 * km/s, its uncertainty converted with it. */
const velocityText = (m: Measurement): string =>
  m.redshift?.kind === "photometric" || (!isCz(m) && Math.abs(m.native.value) > 0.05) ? `z = ${redshiftOf(m)}` : czText(m);

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
  const counts = countBy(
    all.filter((m) => m.quantity === "redshift"),
    (m) => m.source,
  );
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
  const counts = countBy(record.entries, (e) => e.source);
  const n = record.entries.length;
  const title = n === counts.size ? `In ${counted(n, "catalogue")}` : `In ${counted(n, "entry", "entries")} of ${counted(counts.size, "catalogue")}`;
  const detail = [...counts]
    .sort((a, b) => b[1] - a[1])
    .map(([source, k]) => (k === 1 ? name(source) : `${name(source)} ${k.toLocaleString("en-GB")}`))
    .join(", ");
  return { title, detail: `${detail}.` };
}
