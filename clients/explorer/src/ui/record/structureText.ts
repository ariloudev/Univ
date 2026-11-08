// What the structure's record panel shows (see-the-structures), worked out from the record alone:
// the sentence of what it is, its depth between its bounds or its members' distances, where it is
// on the sky, its lengths, its other measures, and the papers it cites and that dispute it. The
// panel draws these; nothing here is drawn.

import type { Cited, Geometry, StructureDetail } from "../../api/structures.ts";
import type { Cosmology } from "../../api/schemas.ts";
import { comovingMpc, velocityMpc } from "../../scene/cosmology.ts";
import { figures, shortDistance } from "../../scene/format.ts";
import { FLOWS } from "../../scene/kinds.ts";
import { toMpc } from "../../scene/units.ts";
import { words } from "../text.ts";
import type { AxisMark, LengthRow } from "./charts.tsx";
import { type PaperName, paperShort } from "./names.ts";
import { type LineData, unitFor } from "./objectRecordText.ts";

const KM_S = "kilometre_per_second";

/** A cited value's number, where it is one. */
const amount = (c: Cited | undefined): number | undefined => (typeof c?.quantity?.value === "number" ? c.quantity.value : undefined);

/** Whether a cited value is a lower limit. */
const atLeast = (c: Cited): boolean => (c.quantity?.uncertainty as { form?: string } | undefined)?.form === "lower_limit";

/** A number in words to `digits` figures, with a true minus sign and a plus where `signed`. */
function signedText(v: number, digits = 4, signed = false): string {
  const text = figures(Math.abs(v), digits);
  return v < 0 ? `−${text}` : signed && v > 0 ? `+${text}` : text;
}

const sentence = (text: string): string => text.charAt(0).toUpperCase() + text.slice(1);

/** A structure's papers by id, each by its first author and year and the rest of its reference. */
export function papersOf(detail: StructureDetail): Map<string, PaperName> {
  return new Map((detail.record.papers ?? []).map((p) => [p.id, paperShort(p.reference)]));
}

/** The paper a value cites by its short name, where the structure cites more than one paper. */
function sourceOf(detail: StructureDetail, c: Cited): string | undefined {
  const papers = papersOf(detail);
  const id = c.cite?.paper;
  return papers.size > 1 && id !== undefined ? (papers.get(id)?.short ?? id) : undefined;
}

/** A value of the record in megaparsecs: a velocity at the comoving distance of z = cz/c, a
 * length in the release cosmology where its paper assumed another. */
function mpcOf(detail: StructureDetail, pointer: string, c: Cited, cosmology: Cosmology | undefined): number | undefined {
  const v = amount(c);
  const unit = c.quantity?.unit ?? "";
  if (v === undefined) return undefined;
  if (unit === KM_S) return cosmology === undefined ? undefined : velocityMpc(v, cosmology);
  if (unit === "dimensionless") return cosmology === undefined ? undefined : comovingMpc(v, cosmology);
  const k = toMpc(unit);
  if (k === undefined) return undefined;
  return detail.rescaled?.find((r) => r.value === pointer)?.rescaled.value ?? v * k;
}

/** The other names, kind, domain and, where it is not plainly observed, the status of a structure. */
export function structureSubtitle(detail: StructureDetail): string {
  const status = detail.status === undefined || detail.status === "observed" ? [] : [words(detail.status)];
  return [...(detail.record.alternative_names ?? []), words(detail.kind), `${words(detail.scale_domain)} domain`, ...status].join(" · ");
}

/** What a structure of each kind is called where its kind's words do not say. */
const NOUNS: Readonly<Record<string, string>> = { basin: "basin of attraction", quasar_group: "large quasar group" };

const withArticle = (noun: string): string => `${/^[aeiou]/i.test(noun) ? "an" : "a"} ${noun}`;

/** A range of megaparsecs in the unit that suits its far end. */
function rangeText(from: number, to: number): string {
  const unit = unitFor(to);
  const at = (mpc: number) => figures((mpc * 1e6) / unit.size, 3);
  return `${at(from)}–${at(to)} ${unit.name}`;
}

/** Where a structure lies, in words: between its depths, out from us, by its members, or at its centre. */
function whereText(detail: StructureDetail, cosmology: Cosmology | undefined): string {
  const band = depthBand(detail, cosmology);
  if (band !== undefined) return band.from === 0 ? ` about us, out to ${shortDistance(band.to)}` : ` ${rangeText(band.from, band.to)} away`;
  const members = memberStrip(detail, cosmology);
  if (members !== undefined) return ` of ${members.values.length} members ${rangeText(Math.min(...members.values), Math.max(...members.values))} away`;
  const c = detail.centre;
  const d = c === null ? 0 : Math.hypot(c.x, c.y, c.z) * (toMpc(c.unit) ?? 1);
  return d > 0 ? ` about ${shortDistance(d)} away` : "";
}

/** One sentence of what kind of structure it is, how far away, and how long its longest length. */
export function structureLede(detail: StructureDetail, cosmology: Cosmology | undefined): string {
  const longest = (sizeView(detail).find((g) => g.unit === "Mpc")?.rows ?? []).filter((r) => !/radius/.test(r.label)).sort((a, b) => b.value - a.value)[0];
  const span = longest === undefined ? "" : `, spanning ${longest.open ? "at least " : ""}${shortDistance(longest.value)}`;
  return sentence(`${withArticle(NOUNS[detail.kind] ?? words(detail.kind))}${whereText(detail, cosmology)}${span}.`);
}

/** The structure's depth: a band from its near bound to its far, in megaparsecs, with the values
 * it gives within them marked, and how the bounds were placed. */
export interface DepthView {
  from: number;
  to: number;
  marks: AxisMark[];
  note: string;
  /** The values marked, by name, which the panel's measures then leave out. */
  used: string[];
}

/** A mark's label: its value's name without the quantity it is. */
const markLabel = (name: string): string => words(name).replace(/ (velocity|distance)$/, "").replace(/redshift/, "z");

/** The values the record gives as a depth within `from` to `to`: a velocity, a redshift or a distance. */
function marksWithin(detail: StructureDetail, from: number, to: number, cosmology: Cosmology | undefined): { marks: AxisMark[]; used: string[] } {
  const marks: AxisMark[] = [];
  const used: string[] = [];
  for (const [name, c] of Object.entries(detail.record.values ?? {})) {
    const unit = c.quantity?.unit;
    const depthLike = unit === KM_S || (unit === "dimensionless" && /redshift$/.test(name)) || (toMpc(unit ?? "") !== undefined && /distance/.test(name));
    const at = depthLike ? mpcOf(detail, `/values/${name}`, c, cosmology) : undefined;
    if (at === undefined || at < from - 1e-6 || at > to + 1e-6) continue;
    marks.push({ value: at, label: markLabel(name) });
    used.push(name);
  }
  return { marks, used };
}

/** A depth bound in megaparsecs, and how it was published. */
function boundOf(detail: StructureDetail, side: "near" | "far", cosmology: Cosmology | undefined): { mpc: number; published: string } | undefined {
  const depth = detail.record.geometry?.SkyRegion?.[side];
  const c = depth?.velocity ?? depth?.distance;
  if (c === undefined) return undefined;
  const pointer = `/geometry/SkyRegion/${side}/${depth?.velocity === undefined ? "distance" : "velocity"}`;
  const mpc = mpcOf(detail, pointer, c, cosmology);
  const v = amount(c);
  return mpc === undefined || v === undefined ? undefined : { mpc, published: depth?.velocity === undefined ? `${figures(v, 4)} Mpc` : `cz ${v.toLocaleString("en-GB")} km/s` };
}

/** How the published values a band's ends were taken from became comoving megaparsecs. */
function placedText(detail: StructureDetail, published: string, cosmology: Cosmology | undefined): string {
  const name = cosmology?.name.replace(/\s*\(.*$/, "") ?? "the release cosmology";
  if (published.startsWith("cz")) return `Published as ${published}, drawn at the comoving distance of z = cz/c under ${name}.`;
  const h = detail.rescaled?.find((r) => r.value.startsWith("/geometry/"))?.native.hubble_constant;
  return h === undefined ? `Published as ${published}.` : `Published as ${published} at H₀ = ${h}, drawn at ${name}'s H₀ = ${cosmology?.hubble_constant ?? "of the release"}.`;
}

/** The band of a structure between a sky region's near and far bounds, or out to a sphere's radius
 * about us; none for a structure with neither. */
export function depthBand(detail: StructureDetail, cosmology: Cosmology | undefined): DepthView | undefined {
  const near = boundOf(detail, "near", cosmology);
  const far = boundOf(detail, "far", cosmology);
  if (near !== undefined && far !== undefined) {
    const published = near.published.startsWith("cz") ? `${near.published.replace(" km/s", "")} to ${far.published.replace("cz ", "")}` : `${near.published.replace(" Mpc", "")}–${far.published}`;
    return { from: near.mpc, to: far.mpc, ...marksWithin(detail, near.mpc, far.mpc, cosmology), note: placedText(detail, published, cosmology) };
  }
  const primitive = detail.record.geometry?.Primitive;
  const radius = primitive?.size?.["radius"];
  if (primitive?.centre?.form !== "observer" || radius === undefined) return undefined;
  const to = mpcOf(detail, "/geometry/Primitive/size/radius", radius, cosmology);
  const v = amount(radius);
  if (to === undefined || v === undefined) return undefined;
  return { from: 0, to, ...marksWithin(detail, 0, to, cosmology), note: placedText(detail, `a radius of ${figures(v, 4)} Mpc about us`, cosmology) };
}

/** A structure placed by its members: each member's comoving distance, with the values the record
 * gives as a depth marked. */
export function memberStrip(detail: StructureDetail, cosmology: Cosmology | undefined): DepthView & { values: number[] } | undefined {
  const members = detail.members?.placed_from_file ?? [];
  if (members.length === 0) return undefined;
  const values = members.map((m) => m.distance_mpc);
  const from = Math.min(...values);
  const to = Math.max(...values);
  const pad = (to - from) / 2;
  return {
    values,
    from,
    to,
    ...marksWithin(detail, from - pad, to + pad, cosmology),
    note: `${members.length.toLocaleString("en-GB")} members, each at the comoving distance of its redshift.`,
  };
}

/** Its place on the sky as lines, and the frame they are in. */
export function skyView(detail: StructureDetail): { lines: LineData[]; note: string } | undefined {
  const region = detail.record.geometry?.SkyRegion;
  if (region !== undefined) return { lines: regionLines(detail, region), note: `${sentence(words(region.frame))} frame.` };
  const position = detail.record.position;
  if (position === null || position === undefined) return undefined;
  return { lines: positionLines(detail, position), note: `${sentence(words(position.frame))} frame.` };
}

type Region = NonNullable<NonNullable<StructureDetail["record"]["geometry"]>["SkyRegion"]>;

/** A sky region as lines: its longitude and latitude ranges, or its centre and size. */
function regionLines(detail: StructureDetail, region: Region): LineData[] {
  const [lon, lat] = axisNames(region.frame);
  const longitude = region.frame.startsWith("equatorial") ? hours : degrees;
  const line = (label: string, cs: readonly Cited[], f: (c: Cited, i: number) => string, join: string): LineData => ({
    label,
    value: cs.map(f).join(join),
    source: cs[0] === undefined ? "" : (sourceOf(detail, cs[0]) ?? ""),
    kind: cs[0]?.kind ?? "",
  });
  if (region.lon !== undefined && region.lat !== undefined)
    return [line(lon, region.lon, (c) => longitude(amount(c) ?? 0), " to "), line(lat, region.lat, (c) => degrees(amount(c) ?? 0, true), " to ")];
  if (region.centre === undefined) return [];
  const centre = line(`${lon}, ${lat}`, region.centre, (c, i) => (i === 0 ? longitude(amount(c) ?? 0) : degrees(amount(c) ?? 0, true)), ", ");
  return region.size === undefined ? [centre] : [centre, line("Across", region.size, (c) => `${atLeast(c) ? "≥ " : ""}${degrees(amount(c) ?? 0)}`, " × ")];
}

/** The names of a frame's longitude and latitude. */
function axisNames(frame: string): [string, string] {
  if (frame.startsWith("equatorial") || frame === "icrs") return ["RA", "Dec"];
  if (frame === "supergalactic") return ["SGL", "SGB"];
  return ["l", "b"];
}

const degrees = (v: number, signed = false): string => `${signedText(v, 4, signed)}°`;

/** Right ascension `deg` in hours and minutes. */
function hours(deg: number): string {
  const total = Math.round((((deg % 360) + 360) % 360) * 4);
  return `${Math.floor(total / 60) % 24}h ${String(total % 60).padStart(2, "0")}m`;
}

type Position = NonNullable<StructureDetail["record"]["position"]>;

/** A published position as lines: its longitude and latitude and velocity, or its Cartesian velocities. */
function positionLines(detail: StructureDetail, position: Position): LineData[] {
  const line = (label: string, value: string, c: Cited): LineData => ({ label, value, source: sourceOf(detail, c) ?? "", kind: c.kind ?? "" });
  if (position.x !== undefined && position.y !== undefined && position.z !== undefined) {
    const xyz = [position.x, position.y, position.z].map((c) => signedText(amount(c) ?? 0, 5)).join(", ");
    return [line(position.frame === "supergalactic" ? "SGX, SGY, SGZ" : "x, y, z", `${xyz} ${unitText(position.x.quantity?.unit)}`, position.x)];
  }
  const lines: LineData[] = [];
  const [lon, lat] = axisNames(position.frame);
  if (position.lon !== undefined && position.lon !== null && position.lat !== undefined && position.lat !== null)
    lines.push(line(`${lon}, ${lat}`, `${degrees(amount(position.lon) ?? 0)}, ${degrees(amount(position.lat) ?? 0, true)}`, position.lon));
  const radial = position.radial?.velocity;
  if (radial !== undefined) lines.push(line("Velocity", valueText(radial), radial));
  return lines;
}

/** A unit as the panel writes it. */
function unitText(unit: string | undefined): string {
  if (unit === KM_S) return "km/s";
  if (unit === "megaparsec") return "Mpc";
  if (unit === "degree" || unit === "dimensionless" || unit === undefined) return "";
  return words(unit);
}

/** A cited value in words: a lower limit marked, in its unit. */
function valueText(c: Cited, rescaled?: number): string {
  const v = rescaled ?? amount(c);
  if (v === undefined) return JSON.stringify(c.quantity?.value ?? null);
  const unit = c.quantity?.unit;
  const number = unit === KM_S ? Math.round(v).toLocaleString("en-GB").replace("-", "−") : signedText(v, 4);
  const text = unit === "degree" ? `${number}°` : `${number} ${unitText(unit)}`.trim();
  return `${atLeast(c) ? "≥ " : ""}${text}`;
}

/** Whether a value is a length of the structure, not a distance to it. */
const isLength = (name: string, c: Cited): boolean =>
  (c.quantity?.unit === "megaparsec" || c.quantity?.unit === KM_S) && /(extent|length|diameter|size|axis|radius|dimension)/.test(name) && !/(to_centre|distance)/.test(name);

/** The lengths of one unit, as bars, and what they were published as where they were rescaled. */
export interface SizeGroup {
  unit: "Mpc" | "km/s";
  rows: LengthRow[];
  note: string | undefined;
}

/** A structure's lengths as bars, one group for each unit, each in the release cosmology. */
export function sizeView(detail: StructureDetail, cosmology?: Cosmology): SizeGroup[] {
  const lengths = Object.entries(detail.record.values ?? {}).filter(([name, c]) => isLength(name, c) && amount(c) !== undefined);
  return (["Mpc", "km/s"] as const).flatMap((unit) => {
    const mine = lengths.filter(([, c]) => (c.quantity?.unit === KM_S ? "km/s" : "Mpc") === unit);
    if (mine.length === 0) return [];
    const rescaled = mine.flatMap(([name]) => detail.rescaled?.filter((r) => r.value === `/values/${name}`) ?? []);
    const rows = mine.map(([name, c]) => {
      const value = detail.rescaled?.find((r) => r.value === `/values/${name}`)?.rescaled.value ?? amount(c) ?? 0;
      return { label: sentence(words(name).replace(/^extent /, "")), value, open: atLeast(c), text: `${atLeast(c) ? "≥ " : ""}${figures(value, 3)}`, kind: c.kind };
    });
    const h = rescaled[0]?.native.hubble_constant;
    const published = rescaled.map((r) => figures(r.native.value, 4)).join(", ");
    const note = h === undefined ? undefined : `Published as ${published} Mpc at H₀ = ${h}, drawn at H₀ = ${cosmology?.hubble_constant ?? "of the release"}.`;
    return [{ unit, rows, note }];
  });
}

/** The structure's other values as lines: each that is neither a length nor marked on its depth. */
export function measureLines(detail: StructureDetail, used: readonly string[]): LineData[] {
  return Object.entries(detail.record.values ?? {})
    .filter(([name, c]) => !isLength(name, c) && !used.includes(name))
    .map(([name, c]) => {
      const rescaled = detail.rescaled?.find((r) => r.value === `/values/${name}`)?.rescaled.value;
      return { label: sentence(words(name)), value: valueText(c, rescaled), source: sourceOf(detail, c) ?? "", kind: c.kind ?? "" };
    });
}

/** The papers that dispute the structure, by their short names. */
export const disputedBy = (detail: StructureDetail): string[] => {
  const papers = papersOf(detail);
  return [...new Set((detail.record.disputes ?? []).map((d) => papers.get(d.cite.paper)?.short ?? d.cite.paper))];
};

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
