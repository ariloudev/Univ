// What the Show drawer offers to turn off (choose-what-is-drawn): each scale domain with the
// sources that place its points, every source once, and the kinds of structure by family.

import { KINDS, STRUCTURE_FAMILIES } from "../../scene/kinds.ts";
import { MASK_BITS } from "../../scene/masks.ts";
import type { Hidden } from "../hidden.ts";
import { sourceShort } from "../record/names.ts";

/** A source of a domain's points: by id, short name and title, and the other domains it places
 * points in. */
export interface ShowSource {
  id: string;
  short: string;
  title: string;
  alsoIn: readonly string[];
}

/** A scale domain: its name, how many points it holds and the sources of its points. */
export interface ShowDomain {
  name: string;
  points: number;
  sources: readonly ShowSource[];
}

/** A family of kinds of structure, each kind with how many structures of it the release holds. */
export interface ShowFamily {
  family: string;
  kinds: readonly { kind: string; count: number }[];
}

export interface ShowChoices {
  domains: readonly ShowDomain[];
  /** Every source once, by id, in the order first met. */
  sources: readonly string[];
  families: readonly ShowFamily[];
}

export interface ChoiceDomain {
  name: string;
  manifest: {
    sources: readonly { index: number; id: string; title: string; role: string }[];
    nodes: readonly { points: number }[];
  };
}

/** The sources a domain's points can be turned off by: of an index a mask can hold, and not one
 * that only places points on the sky (it decides no point's distance, so turning it off would hide
 * nothing). */
const placing = (d: ChoiceDomain) => d.manifest.sources.filter((s) => s.role !== "position" && s.index < MASK_BITS);

/** The scale domains from the smallest scale up; one not named here comes after them. */
const SCALE_ORDER = ["stars", "nearby_galaxies", "cosmic_web"];
const scaleRank = (name: string) => (SCALE_ORDER.includes(name) ? SCALE_ORDER.indexOf(name) : SCALE_ORDER.length);

/** What `domains` and `structures` offer to turn off: the domains from the smallest scale up,
 * the rest each in the order first met. */
export function choicesOf(unordered: readonly ChoiceDomain[], structures: readonly { kind: string }[]): ShowChoices {
  const domains = [...unordered].sort((a, b) => scaleRank(a.name) - scaleRank(b.name));
  const holders = new Map<string, string[]>();
  for (const d of domains) for (const s of placing(d)) holders.set(s.id, [...(holders.get(s.id) ?? []), d.name]);
  return {
    domains: domains.map((d) => ({
      name: d.name,
      points: d.manifest.nodes.reduce((sum, n) => sum + n.points, 0),
      sources: placing(d).map((s) => ({
        id: s.id,
        short: sourceShort(s.id, s.title),
        title: s.title,
        alsoIn: (holders.get(s.id) ?? []).filter((other) => other !== d.name),
      })),
    })),
    sources: [...holders.keys()],
    families: familiesOf(structures),
  };
}

/** The kinds of `structures` with their counts, by family, each family in its own order and any
 * kind of none under "other"; a family with no kind left out. */
function familiesOf(structures: readonly { kind: string }[]): ShowFamily[] {
  const counts = new Map<string, number>();
  for (const s of structures) counts.set(s.kind, (counts.get(s.kind) ?? 0) + 1);
  const known = new Set(STRUCTURE_FAMILIES.flatMap((f) => f.kinds));
  const withCounts = (kinds: readonly string[]) => kinds.flatMap((kind) => (counts.has(kind) ? [{ kind, count: counts.get(kind) ?? 0 }] : []));
  return [...STRUCTURE_FAMILIES, { family: "other", kinds: [...counts.keys()].filter((k) => !known.has(k)) }]
    .map((f) => ({ family: f.family, kinds: withCounts(f.kinds) }))
    .filter((f) => f.kinds.length > 0);
}

/** How many of a group are on, of how many. */
export interface Tally {
  on: number;
  of: number;
}

const tally = (names: readonly string[], off: ReadonlySet<string>): Tally => ({ on: names.filter((n) => !off.has(n)).length, of: names.length });

/** How much of each group is on: the domains, sources and kinds of value of the points, and the
 * kinds of structure. */
export function summaryOf(choices: ShowChoices, hidden: Hidden): Record<"domains" | "sources" | "kinds" | "structures", Tally> {
  return {
    domains: tally(
      choices.domains.map((d) => d.name),
      hidden.domain,
    ),
    sources: tally(choices.sources, hidden.source),
    kinds: tally(KINDS, hidden.kind),
    structures: tally(
      choices.families.flatMap((f) => f.kinds.map((k) => k.kind)),
      hidden.structure,
    ),
  };
}
