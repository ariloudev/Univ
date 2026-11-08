// What the structure's record panel shows (see-the-structures), worked out from the record: the
// sentence of what it is, its depth between the bounds Univ Server placed it at or its members'
// distances, where it is on the sky, its lengths, its other measures, and the papers it cites and
// that dispute it. The panel draws these; nothing here is drawn.

import type { Cosmology } from "../../api/schemas.ts";
import type { Cited, Geometry, StructureDetail, StructureRecord } from "../../api/structures.ts";
import { comovingMpc, velocityMpc } from "../../scene/cosmology.ts";
import { figures, pcUnit, shortDistance, whole } from "../../scene/format.ts";
import { FLOWS } from "../../scene/kinds.ts";
import { placed } from "../../scene/structures.ts";
import { toMpc } from "../../scene/units.ts";
import { norm } from "../../scene/vec.ts";
import { words } from "../text.ts";
import type { AxisMark, LengthRow } from "./charts.tsx";
import { type PaperName, paperShort } from "./names.ts";
import type { LineData } from "./objectRecordText.ts";
import { type Uncertainty, sentence, shortWithUncertainty, withArticle } from "./wording.ts";

const KM_S = "kilometre_per_second";

/** A cited value's number, where it is one. */
const amount = (c: Cited | undefined): number | undefined => (typeof c?.quantity?.value === "number" ? c.quantity.value : undefined);

const uncertaintyOf = (c: Cited): Uncertainty | undefined => c.quantity?.uncertainty as Uncertainty | undefined;

/** Whether a cited value is a lower limit. */
const atLeast = (c: Cited): boolean => uncertaintyOf(c)?.form === "lower_limit";

/** What a value of a structure's record is to its panel: one of its lengths, a depth that can be
 * marked on it, or another measure. Taken from its name and unit until the structure files say. */
export type Role = "length" | "depth" | "measure";

export function roleOf(name: string, c: Cited): Role {
  const unit = c.quantity?.unit;
  const lengthUnit = unit === "megaparsec" || unit === KM_S;
  if (lengthUnit && /(extent|length|diameter|size|axis|radius|dimension)/i.test(name) && !/(to_centre|distance)/i.test(name)) return "length";
  if (unit === KM_S && /(^|_)(velocity|cz)$/i.test(name) && !/(infall|dispersion|flow|rotation|peculiar)/i.test(name)) return "depth";
  if (unit === "dimensionless" && /redshift$/i.test(name)) return "depth";
  if (toMpc(unit ?? "") !== undefined && /distance/i.test(name)) return "depth";
  return "measure";
}

/** The value of `pointer` in the release cosmology, where its paper assumed another. */
const rescaledOf = (s: StructureRecord, pointer: string) => s.rescaled?.find((r) => r.value === pointer);

/** A structure's papers by id, each by its first author and year and the rest of its reference. */
export function papersOf(s: StructureRecord): Map<string, PaperName> {
  return new Map((s.record.papers ?? []).map((p) => [p.id, paperShort(p.reference)]));
}

/** Names the paper a value cites by its short name, where the structure cites more than one. */
function sourcer(s: StructureRecord): (c: Cited) => string {
  const papers = papersOf(s);
  return (c) => {
    const id = c.cite?.paper;
    return papers.size > 1 && id !== undefined ? (papers.get(id)?.short ?? id) : "";
  };
}

/** A depth value of the record in megaparsecs: a velocity at the comoving distance of z = cz/c,
 * a redshift at its comoving distance, a distance in the release cosmology. */
function depthMpc(s: StructureRecord, name: string, c: Cited, cosmology: Cosmology | undefined): number | undefined {
  const v = amount(c);
  const unit = c.quantity?.unit ?? "";
  if (v === undefined) return undefined;
  if (unit === KM_S) return cosmology === undefined ? undefined : velocityMpc(v, cosmology);
  if (unit === "dimensionless") return cosmology === undefined ? undefined : comovingMpc(v, cosmology);
  const k = toMpc(unit);
  return k === undefined ? undefined : (rescaledOf(s, `/values/${name}`)?.rescaled.value ?? v * k);
}

/** The other names, kind, domain and, where it is not plainly observed, the status of a structure. */
export function structureSubtitle(s: StructureRecord): string {
  const status = s.status === undefined || s.status === "observed" ? [] : [words(s.status)];
  return [...(s.record.alternative_names ?? []), words(s.kind), `${words(s.scale_domain)} domain`, ...status].join(" · ");
}

/** What a structure of each kind is called where its kind's words do not say. */
const NOUNS: Readonly<Record<string, string>> = { basin: "basin of attraction", quasar_group: "large quasar group" };

/** A range of megaparsecs in the unit that suits its far end. */
function rangeText(from: number, to: number): string {
  const unit = pcUnit(to);
  const at = (mpc: number) => figures((mpc * 1e6) / unit.size, 3);
  return `${at(from)}–${at(to)} ${unit.name}`;
}

/** What the panel draws of a structure, worked out once for its sentence and its parts. */
export interface StructureParts {
  band: DepthView | undefined;
  members: MemberView | undefined;
  size: SizeGroup[];
}

export const structureParts = (s: StructureRecord, cosmology: Cosmology | undefined): StructureParts => ({
  band: depthBand(s, cosmology),
  members: memberStrip(s, cosmology),
  size: sizeView(s, cosmology),
});

/** Where a structure lies, in words: between its depths, out from us, by its members, or at its centre. */
function whereText(s: StructureDetail, { band, members }: StructureParts): string {
  if (band !== undefined) return band.from === 0 ? ` about us, out to ${shortDistance(band.to)}` : ` ${rangeText(band.from, band.to)} away`;
  if (members !== undefined) return ` of ${members.values.length.toLocaleString("en-GB")} members ${rangeText(members.from, members.to)} away`;
  const centre = placed(s)?.centre;
  const d = centre === undefined ? 0 : norm(centre);
  return d > 0 ? ` about ${shortDistance(d)} away` : "";
}

/** One sentence of what kind of structure it is, how far away, and how long its longest length. */
export function structureLede(s: StructureDetail, parts: StructureParts): string {
  const lengths = parts.size.find((g) => g.unit === "Mpc")?.rows ?? [];
  const longest = lengths.filter((r) => !/radius/i.test(r.label)).sort((a, b) => b.value - a.value)[0];
  const span = longest === undefined ? "" : `, spanning ${longest.open ? "at least " : ""}${shortDistance(longest.value)}`;
  return sentence(`${withArticle(NOUNS[s.kind] ?? words(s.kind))}${whereText(s, parts)}${span}.`);
}

/** The structure's depth: a band from its near bound to its far, in megaparsecs, with the depths
 * its record gives within them marked, and how the bounds were placed. */
export interface DepthView {
  from: number;
  to: number;
  marks: AxisMark[];
  note: string;
  /** The values marked, by name, which the panel's measures then leave out. */
  used: string[];
}

/** A structure placed by its members: each member's distance, as a depth view. */
export type MemberView = DepthView & { values: number[] };

/** A mark's label: its value's name without the quantity it is. */
const markLabel = (name: string): string => words(name).replace(/ (velocity|distance)$/, "").replace(/redshift/, "z");

/** The depths the record gives within `from` to `to`, each marked where it lies. */
function marksWithin(s: StructureRecord, from: number, to: number, cosmology: Cosmology | undefined): { marks: AxisMark[]; used: string[] } {
  const marks: AxisMark[] = [];
  const used: string[] = [];
  for (const [name, c] of Object.entries(s.record.values ?? {})) {
    const at = roleOf(name, c) === "depth" ? depthMpc(s, name, c, cosmology) : undefined;
    if (at === undefined || at < from - 1e-6 || at > to + 1e-6) continue;
    marks.push({ value: at, label: markLabel(name) });
    used.push(name);
  }
  return { marks, used };
}

/** A published depth bound as its paper gave it. */
function publishedBound(c: Cited | undefined): { text: string; velocity: boolean } | undefined {
  const v = amount(c);
  if (c === undefined || v === undefined) return undefined;
  return c.quantity?.unit === KM_S ? { text: whole(v), velocity: true } : { text: figures(v, 4), velocity: false };
}

/** How a sky region's near and far bounds were published, and how Univ Server placed them. */
function regionNote(s: StructureRecord): string {
  const region = s.record.geometry?.SkyRegion;
  const near = publishedBound(region?.near?.velocity ?? region?.near?.distance);
  const far = publishedBound(region?.far?.velocity ?? region?.far?.distance);
  const published = near === undefined || far === undefined ? "" : near.velocity ? `Published as cz ${near.text} to ${far.text} km/s; ` : `Published as ${near.text}–${far.text} Mpc; `;
  const method = s.derivation?.distance?.method.split(";")[0];
  return `${published}${method === undefined ? "placed by Univ Server" : `placed by ${method}`}.`;
}

/** How a sphere's radius was published, and its value in the release cosmology. */
function radiusNote(s: StructureRecord, radius: Cited | undefined, cosmology: Cosmology | undefined): string {
  const r = rescaledOf(s, "/geometry/Primitive/size/radius");
  const v = amount(radius);
  if (r === undefined) return v === undefined ? "A sphere about us." : `Published as a radius of ${figures(v, 4)} Mpc about us.`;
  const drawn = cosmology === undefined ? "" : `, drawn at H₀ = ${cosmology.hubble_constant}`;
  return `Published as a radius of ${figures(r.native.value, 4)} Mpc about us at H₀ = ${r.native.hubble_constant}${drawn}.`;
}

/** The band of a structure between the near and far bounds Univ Server placed its sky region at,
 * or out to the radius of its sphere about us; none for a structure drawn otherwise. */
export function depthBand(s: StructureRecord, cosmology: Cosmology | undefined): DepthView | undefined {
  const shape = s.shape;
  const k = shape === null ? undefined : toMpc(shape.unit);
  if (shape === null || k === undefined) return undefined;
  const g = shape.geometry;
  if (g.kind === "sky_region") return { from: g.near * k, to: g.far * k, ...marksWithin(s, g.near * k, g.far * k, cosmology), note: regionNote(s) };
  if (g.kind !== "primitive" || g.primitive.form !== "sphere" || norm(g.primitive.centre) !== 0) return undefined;
  const to = g.primitive.radius * k;
  return { from: 0, to, ...marksWithin(s, 0, to, cosmology), note: radiusNote(s, s.record.geometry?.Primitive?.size?.["radius"], cosmology) };
}

/** A structure placed by its members: each member's comoving distance, with the depths the record
 * gives about them marked. */
export function memberStrip(s: StructureRecord, cosmology: Cosmology | undefined): MemberView | undefined {
  const members = s.members?.placed_from_file ?? [];
  if (members.length === 0) return undefined;
  const values = members.map((m) => m.distance_mpc);
  const from = Math.min(...values);
  const to = Math.max(...values);
  const pad = (to - from) / 2;
  return {
    values,
    from,
    to,
    ...marksWithin(s, from - pad, to + pad, cosmology),
    note: `${members.length.toLocaleString("en-GB")} members, each at the comoving distance of its redshift.`,
  };
}

/** Its place on the sky as lines, and the frame they are in. */
export function skyView(s: StructureRecord): { lines: LineData[]; note: string } | undefined {
  const source = sourcer(s);
  const region = s.record.geometry?.SkyRegion;
  if (region !== undefined) return { lines: regionLines(region, source), note: `${sentence(words(region.frame))} frame.` };
  const position = s.record.position;
  if (position === null || position === undefined) return undefined;
  return { lines: positionLines(position, source), note: `${sentence(words(position.frame))} frame.` };
}

type Region = NonNullable<NonNullable<StructureRecord["record"]["geometry"]>["SkyRegion"]>;

/** A frame's names for its longitude and latitude, and how its longitude is written: right
 * ascension in hours, any other longitude in degrees. */
function axesOf(frame: string): { lon: string; lat: string; longitude: (v: number) => string } {
  if (frame.startsWith("equatorial") || frame === "icrs") return { lon: "RA", lat: "Dec", longitude: hours };
  if (frame === "supergalactic") return { lon: "SGL", lat: "SGB", longitude: degrees };
  return { lon: "l", lat: "b", longitude: degrees };
}

const degrees = (v: number): string => `${figures(v, 4)}°`;
const latitude = (v: number): string => `${v > 0 ? "+" : ""}${degrees(v)}`;

/** Right ascension `deg` in hours and minutes. */
function hours(deg: number): string {
  const total = Math.round((((deg % 360) + 360) % 360) * 4);
  return `${Math.floor(total / 60) % 24}h ${String(total % 60).padStart(2, "0")}m`;
}

/** A line of cited values, worded each by `f` and joined by `join`, from the first one's paper. */
function citedLine(label: string, cs: readonly Cited[], f: (v: number, i: number) => string, join: string, source: (c: Cited) => string): LineData {
  const first = cs[0];
  return { label, value: cs.map((c, i) => f(amount(c) ?? 0, i)).join(join), source: first === undefined ? "" : source(first), kind: first?.kind ?? "" };
}

/** A sky region as lines: its longitude and latitude ranges, or its centre and size. */
function regionLines(region: Region, source: (c: Cited) => string): LineData[] {
  const { lon, lat, longitude } = axesOf(region.frame);
  if (region.lon !== undefined && region.lat !== undefined)
    return [citedLine(lon, region.lon, longitude, " to ", source), citedLine(lat, region.lat, latitude, " to ", source)];
  if (region.centre === undefined) return [];
  const centre = citedLine(`${lon}, ${lat}`, region.centre, (v, i) => (i === 0 ? longitude(v) : latitude(v)), ", ", source);
  if (region.size === undefined) return [centre];
  const across = citedLine("Across", region.size, degrees, " × ", source);
  return [centre, { ...across, value: region.size.some(atLeast) ? `≥ ${across.value}` : across.value }];
}

type Position = NonNullable<StructureRecord["record"]["position"]>;

/** A published position as lines: its longitude, latitude and velocity, or its Cartesian velocities. */
function positionLines(position: Position, source: (c: Cited) => string): LineData[] {
  const { x, y, z } = position;
  if (x !== undefined && y !== undefined && z !== undefined)
    return [citedLine(position.frame === "supergalactic" ? "SGX, SGY, SGZ" : "x, y, z", [x, y, z], (v) => `${whole(v)}`, ", ", source)].map((l) => ({
      ...l,
      value: `${l.value} ${unitText(x.quantity?.unit)}`.trim(),
    }));
  const lines: LineData[] = [];
  const { lon, lat, longitude } = axesOf(position.frame);
  if (position.lon !== undefined && position.lon !== null && position.lat !== undefined && position.lat !== null)
    lines.push(citedLine(`${lon}, ${lat}`, [position.lon, position.lat], (v, i) => (i === 0 ? longitude(v) : latitude(v)), ", ", source));
  const radial = position.radial?.velocity;
  if (radial !== undefined) lines.push({ label: "Velocity", value: valueText(radial), source: source(radial), kind: radial.kind ?? "" });
  return lines;
}

/** A unit as the panel writes it. */
function unitText(unit: string | undefined): string {
  if (unit === KM_S) return "km/s";
  if (unit === "megaparsec") return "Mpc";
  if (unit === "degree" || unit === "dimensionless" || unit === undefined) return "";
  return words(unit);
}

/** A cited value in short, with its uncertainty as published, in its unit; `rescaled` where its
 * paper assumed another Hubble constant. */
function valueText(c: Cited, rescaled?: number): string {
  const v = rescaled ?? amount(c);
  if (v === undefined) return JSON.stringify(c.quantity?.value ?? null);
  const unit = c.quantity?.unit;
  const number = shortWithUncertainty(unit === KM_S ? whole(v) : figures(v, 4), uncertaintyOf(c), rescaled === undefined ? 1 : rescaled / (amount(c) ?? rescaled));
  return unit === "degree" ? `${number}°` : `${number} ${unitText(unit)}`.trim();
}

/** The lengths of one unit, as bars, and what they were published as where they were rescaled. */
export interface SizeGroup {
  unit: "Mpc" | "km/s";
  rows: LengthRow[];
  note: string | undefined;
}

/** A structure's lengths as bars, one group for each unit, each in the release cosmology. */
export function sizeView(s: StructureRecord, cosmology: Cosmology | undefined): SizeGroup[] {
  const lengths = Object.entries(s.record.values ?? {}).filter(([name, c]) => roleOf(name, c) === "length" && amount(c) !== undefined);
  return (["Mpc", "km/s"] as const).flatMap((unit) => {
    const mine = lengths.filter(([, c]) => (c.quantity?.unit === KM_S ? "km/s" : "Mpc") === unit);
    if (mine.length === 0) return [];
    const rescaled = mine.flatMap(([name]) => rescaledOf(s, `/values/${name}`) ?? []);
    const rows = mine.map(([name, c]) => {
      const value = rescaledOf(s, `/values/${name}`)?.rescaled.value ?? amount(c) ?? 0;
      return { label: sentence(words(name).replace(/^extent /, "")), value, open: atLeast(c), text: `${atLeast(c) ? "≥ " : ""}${figures(value, 3)}`, kind: c.kind };
    });
    const h = rescaled[0]?.native.hubble_constant;
    const published = rescaled.map((r) => figures(r.native.value, 4)).join(", ");
    const drawn = cosmology === undefined ? "" : `, drawn at H₀ = ${cosmology.hubble_constant}`;
    return [{ unit, rows, note: h === undefined ? undefined : `Published as ${published} Mpc at H₀ = ${h}${drawn}.` }];
  });
}

/** The structure's other values as lines: each that is neither a length nor marked on its depth. */
export function measureLines(s: StructureRecord, used: readonly string[]): LineData[] {
  const source = sourcer(s);
  return Object.entries(s.record.values ?? {})
    .filter(([name, c]) => roleOf(name, c) !== "length" && !used.includes(name))
    .map(([name, c]) => ({ label: sentence(words(name)), value: valueText(c, rescaledOf(s, `/values/${name}`)?.rescaled.value), source: source(c), kind: c.kind ?? "" }));
}

/** The papers that dispute the structure, by their short names. */
export function disputedBy(s: StructureRecord): string[] {
  const papers = papersOf(s);
  return [...new Set((s.record.disputes ?? []).map((d) => papers.get(d.cite.paper)?.short ?? d.cite.paper))];
}

/** What the symbol of an attractor or a repeller means; none for any other kind. */
export function flowText(kind: string): string | undefined {
  const flow = FLOWS[kind];
  if (flow === undefined) return undefined;
  const way = flow === "in" ? "in toward it, where its reconstructed flow converges" : "out from it, where its reconstructed flow diverges";
  return `Its sphere is a symbol of a fixed size on the view, not its extent; its arrows point ${way}.`;
}

/** A geometry's kind in words: a primitive by its form. */
const shapeKind = (geometry: Geometry): string => words(geometry.kind === "primitive" ? geometry.primitive.form : geometry.kind);

/** How a structure's shape was placed, in words, with the parts of its record it was derived from
 * where `from` asks for them. */
export function matchedText(detail: StructureDetail, approximate: boolean, from = true): string {
  if (detail.shape === null && detail.bounds === null) return "not placed: no shape and no bounding box, so it is listed but not drawn";
  const parts = from ? detail.derivation?.from?.map((p) => words(p.replace(/^\//, "").replaceAll("/", " "))).join("; ") : undefined;
  const shape = detail.shape === null ? "no placed shape" : `a ${shapeKind(detail.shape.geometry)} in ${detail.shape.frame}`;
  const drawn = approximate ? ", drawn as its bounding box (approximate)" : "";
  return `${shape}${parts === undefined || parts === "" ? "" : `, derived from ${parts}`}${drawn}`;
}
