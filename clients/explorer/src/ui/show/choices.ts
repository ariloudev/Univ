// What the Show drawer offers to turn off (choose-what-is-drawn): the scale domains from the
// smallest scale up, each source once with the domains whose points it places, and the kinds of
// structure by family.

import { MPC_PER } from "../../catalogue/tiles.ts";
import { KINDS, STRUCTURE_FAMILIES } from "../../scene/kinds.ts";
import { MASK_BITS } from "../../scene/masks.ts";
import { TRACERS_DOMAIN, TRACER_KINDS } from "../../scene/tracers.ts";
import type { Hidden } from "../hidden.ts";
import { sourceShort } from "../record/names.ts";

/** A source of the drawn points: by id, short name and title, and the domains whose points it
 * places, from the smallest scale up. */
export interface ShowSource {
  id: string;
  short: string;
  title: string;
  domains: readonly string[];
}

export interface ShowChoices {
  /** The domains from the smallest scale up, each with how many points it holds. */
  domains: readonly { name: string; points: number }[];
  /** Each source once, grouped by the smallest domain whose points it places. */
  sources: readonly ShowSource[];
  /** The kinds of structure by family, each with how many structures of it the release holds. */
  families: readonly { family: string; kinds: readonly { kind: string; count: number }[] }[];
  /** The kinds of object of the Galaxy's tracers, where the release holds them; none else. */
  objectKinds: readonly string[];
  /** The names of the domains, sources and kinds of structure, for turning all of one on or off. */
  domainNames: readonly string[];
  sourceIds: readonly string[];
  structureKinds: readonly string[];
}

interface ChoiceDomain {
  name: string;
  manifest: {
    unit: keyof typeof MPC_PER;
    radius: number;
    points: number;
    sources: readonly { index: number; id: string; title: string; role: string }[];
  };
}

/** A domain's radius in megaparsecs, by which the domains are put in order of scale. */
const scaleOf = (d: ChoiceDomain) => d.manifest.radius * MPC_PER[d.manifest.unit];

/** What `domains` and `structures` offer to turn off. A domain's sources are those of an index a
 * mask can hold, and not one that only places points on the sky: it decides no point's distance,
 * so turning it off would hide nothing. */
export function choicesOf(unordered: readonly ChoiceDomain[], structures: readonly { kind: string }[]): ShowChoices {
  const domains = [...unordered].sort((a, b) => scaleOf(a) - scaleOf(b));
  const sources = new Map<string, ShowSource & { domains: string[] }>();
  for (const d of domains) {
    for (const s of d.manifest.sources) {
      if (s.role === "position" || s.index >= MASK_BITS) continue;
      const known = sources.get(s.id);
      if (known === undefined) sources.set(s.id, { id: s.id, short: sourceShort(s.id, s.title), title: s.title, domains: [d.name] });
      else known.domains.push(d.name);
    }
  }
  const families = familiesOf(structures);
  return {
    domains: domains.map((d) => ({ name: d.name, points: d.manifest.points })),
    sources: [...sources.values()],
    families,
    objectKinds: domains.some((d) => d.name === TRACERS_DOMAIN) ? TRACER_KINDS : [],
    domainNames: domains.map((d) => d.name),
    sourceIds: [...sources.keys()],
    structureKinds: families.flatMap((f) => f.kinds.map((k) => k.kind)),
  };
}

/** The kinds of `structures` with their counts, by family, each family in its own order and any
 * kind of none under "other"; a family with no kind left out. */
function familiesOf(structures: readonly { kind: string }[]): ShowChoices["families"] {
  const counts = new Map<string, number>();
  for (const s of structures) counts.set(s.kind, (counts.get(s.kind) ?? 0) + 1);
  const known = new Set(STRUCTURE_FAMILIES.flatMap((f) => f.kinds));
  const withCounts = (kinds: readonly string[]) => kinds.flatMap((kind) => {
    const count = counts.get(kind);
    return count === undefined ? [] : [{ kind, count }];
  });
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

/** How much of each group this release offers is on: the domains, sources, kinds of value and
 * kinds of object of the points, and the kinds of structure. */
export function summaryOf(choices: ShowChoices, hidden: Hidden): Record<"domains" | "sources" | "kinds" | "objects" | "structures", Tally> {
  return {
    domains: tally(choices.domainNames, hidden.domain),
    sources: tally(choices.sourceIds, hidden.source),
    kinds: tally(KINDS, hidden.kind),
    objects: tally(choices.objectKinds, hidden.object),
    structures: tally(choices.structureKinds, hidden.structure),
  };
}

/** Whether a group this release offers is off. */
export const anyOff = (summary: ReturnType<typeof summaryOf>): boolean => Object.values(summary).some((t) => t.on < t.of);
