// The Structures drawer's depth index (see-the-structures): each structure listed outward from
// Earth, placed on one logarithmic axis of distance that every row shares, and known as observed,
// a statistical claim, or reconstructed.

import type { StructureSummary } from "../../api/structures.ts";
import { figures, whole } from "../../scene/format.ts";
import { structureCss } from "../../scene/kinds.ts";
import { norm } from "../../scene/vec.ts";
import type { Structures } from "../useStructures.ts";
import { words } from "../text.ts";

/** How a structure is known, as its record says. */
export type Known = "observed" | "statistical_claim" | "reconstructed";

const KNOWN: readonly Known[] = ["observed", "statistical_claim", "reconstructed"];

/** The words for how a structure is known. */
export const KNOWN_WORDS: Record<Known, string> = { observed: "observed", statistical_claim: "statistical claim", reconstructed: "reconstructed" };

/** One row of the index: a structure, its kind's colour, how it is known, and the distance from
 * Earth to its centre as the view places it, megaparsecs, where it is placed. */
export interface DepthRow {
  id: string;
  name: string;
  kind: string;
  colour: string;
  known: Known;
  distance: number | undefined;
}

/** The rows of `shown`, named and placed as `structures` draws them. A record that says nothing of how the
 * structure is known, or says what the explorer does not know, reads as observed. */
export function depthRows(shown: readonly StructureSummary[], structures: Pick<Structures, "details" | "drawings">): DepthRow[] {
  return shown.map((s) => {
    const drawing = structures.drawings.get(s.id);
    const status = structures.details.get(s.id)?.status;
    const known = KNOWN.find((k) => k === status) ?? "observed";
    const centre = drawing?.centre;
    // The name as drawn, "(at least)" after it where its shape is a lower limit on its extent.
    return { id: s.id, name: drawing?.name ?? s.name, kind: s.kind, colour: structureCss(s.kind), known, distance: centre === undefined ? undefined : norm(centre) };
  });
}

/** Within this distance of Earth, megaparsecs, a structure is centred on it. */
const AROUND_US = 0.5;

/** A distance in whole megaparsecs below 1,000 Mpc and in gigaparsecs to three figures from
 * there; "around us" for a structure centred on Earth, and "not placed" for none. */
export function distanceText(mpc: number | undefined): string {
  if (mpc === undefined) return "not placed";
  if (mpc < AROUND_US) return "around us";
  return Math.round(mpc) < 1000 ? `${whole(mpc)} Mpc` : `${figures(mpc / 1000)} Gpc`;
}

/** The axis's ends, as powers of ten of megaparsecs: 10 Mpc to 10 Gpc. */
const NEAR = 1;
const FAR = 4;

/** The axis's labelled ticks, megaparsecs, with their words. */
export const DEPTH_TICKS: readonly { mpc: number; text: string }[] = [
  { mpc: 10, text: "10 Mpc" },
  { mpc: 100, text: "100 Mpc" },
  { mpc: 1000, text: "1 Gpc" },
  { mpc: 10_000, text: "10 Gpc" },
];

/** Where `mpc` lies along an axis `width` long: logarithmically from 10 Mpc to 10 Gpc, held at
 * its ends. */
export function depthX(mpc: number, width: number): number {
  const at = (Math.log10(Math.max(mpc, 10 ** NEAR)) - NEAR) / (FAR - NEAR);
  return Math.min(1, at) * width;
}

/** The headings the index groups its rows under, each with the distances it holds. */
const BANDS: readonly { heading: string; holds: (mpc: number | undefined) => boolean }[] = [
  { heading: "Within 100 Mpc", holds: (d) => d !== undefined && d < 100 },
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
