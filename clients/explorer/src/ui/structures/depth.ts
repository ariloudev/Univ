// The Structures drawer's depth index (see-the-structures): each structure listed outward from
// Earth, placed on one logarithmic axis of distance that every row shares, and known as observed,
// a statistical claim, reconstructed, or a model.

import type { StructureSummary } from "../../api/structures.ts";
import { figures, shortDistance, whole } from "../../scene/format.ts";
import { structureCss } from "../../scene/kinds.ts";
import { norm } from "../../scene/vec.ts";
import type { Structures } from "../useStructures.ts";
import { words } from "../text.ts";

/** How a structure is known, as its record says. */
export type Known = "observed" | "statistical_claim" | "reconstructed" | "model";

/** The ways a structure is known, in the legend's order. */
export const KNOWN: readonly Known[] = ["observed", "statistical_claim", "reconstructed", "model"];

/** One row of the index: a structure, its kind's colour, how it is known where its record says,
 * and the distance from Earth to its centre as the view places it, megaparsecs, where it is
 * placed. */
export interface DepthRow {
  id: string;
  name: string;
  kind: string;
  colour: string;
  known: Known | undefined;
  distance: number | undefined;
}

/** The rows of `shown`, named and placed as `structures` draws them. A structure whose record is
 * not read, or says nothing the explorer knows of how it is known, is known in no way at all:
 * never taken for observed. */
export function depthRows(shown: readonly StructureSummary[], structures: Pick<Structures, "details" | "drawings">): DepthRow[] {
  return shown.map((s) => {
    const drawing = structures.drawings.get(s.id);
    const status = structures.details.get(s.id)?.status;
    const known = KNOWN.find((k) => k === status);
    const centre = drawing?.centre;
    // The name the view gives the structure, "(at least)" after it where its shape is a lower
    // limit on its extent.
    return { id: s.id, name: drawing?.name ?? s.name, kind: s.kind, colour: structureCss(s.kind), known, distance: centre === undefined ? undefined : norm(centre) };
  });
}

/** Within this distance of Earth, megaparsecs, a structure is centred on it: a parsec, well
 * within the nearest structure placed apart from Earth, a model arm kiloparsecs away. */
const AROUND_US = 1e-6;

/** A distance in kiloparsecs to three figures below 1 Mpc, in whole megaparsecs from there to
 * below 1,000 Mpc and in gigaparsecs to three figures from there, as the headings divide them;
 * "around us" for a structure centred on Earth, and "not placed" for none. */
export function distanceText(mpc: number | undefined): string {
  if (mpc === undefined) return "not placed";
  if (mpc < AROUND_US) return "around us";
  if (mpc < 1) return `${figures(mpc * 1000)} kpc`;
  return mpc < 1000 ? `${whole(mpc)} Mpc` : `${figures(mpc / 1000)} Gpc`;
}

/** The axis's ends, as powers of ten of megaparsecs: 1 kpc to 10 Gpc. */
const NEAR = -3;
const FAR = 4;

/** The axis's labelled ticks, megaparsecs, with their words: every hundredfold from 1 kpc. */
export const DEPTH_TICKS: readonly { mpc: number; text: string }[] = [1e-3, 0.1, 10, 1000].map((mpc) => ({ mpc, text: shortDistance(mpc) }));

/** Where `mpc` lies along an axis `width` long: logarithmically from 1 kpc to 10 Gpc, held at
 * its ends. */
export function depthX(mpc: number, width: number): number {
  const at = (Math.log10(Math.max(mpc, 10 ** NEAR)) - NEAR) / (FAR - NEAR);
  return Math.min(1, at) * width;
}

/** The headings the index groups its rows under, each with the distances it holds. */
const BANDS: readonly { heading: string; holds: (mpc: number | undefined) => boolean }[] = [
  { heading: "Within 30 kpc", holds: (d) => d !== undefined && d < 0.03 },
  { heading: "30 kpc to 100 Mpc", holds: (d) => d !== undefined && d >= 0.03 && d < 100 },
  { heading: "100 Mpc to 1 Gpc", holds: (d) => d !== undefined && d >= 100 && d < 1000 },
  { heading: "1 to 10 Gpc", holds: (d) => d !== undefined && d >= 1000 },
  { heading: "Not placed", holds: (d) => d === undefined },
];

const nearestFirst = (a: DepthRow, b: DepthRow): number => (a.distance ?? Number.POSITIVE_INFINITY) - (b.distance ?? Number.POSITIVE_INFINITY) || a.name.localeCompare(b.name);

/** `rows` under the headings that hold any of them, nearest first under each. */
export const banded = (rows: readonly DepthRow[]): { heading: string; rows: DepthRow[] }[] =>
  BANDS.map((band) => ({ heading: band.heading, rows: rows.filter((r) => band.holds(r.distance)).sort(nearestFirst) })).filter((band) => band.rows.length > 0);

/** `rows` by name, A to Z. */
export const byName = (rows: readonly DepthRow[]): DepthRow[] => [...rows].sort((a, b) => a.name.localeCompare(b.name));

/** The rows whose name or kind, in words, holds `text`, whatever its case; all for no text. */
export function matching(rows: readonly DepthRow[], text: string): DepthRow[] {
  const sought = text.trim().toLowerCase();
  if (sought === "") return [...rows];
  return rows.filter((r) => r.name.toLowerCase().includes(sought) || words(r.kind).toLowerCase().includes(sought));
}
