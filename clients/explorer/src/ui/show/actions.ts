// The changes each of the Show drawer's controls makes (choose-what-is-drawn): a domain turned on
// with its sources, only one source or kind of structure, All and None, and everything at once.

import { KINDS } from "../../scene/kinds.ts";
import { type Change, type Group, GROUPS, type Hidden } from "../hidden.ts";
import type { ShowChoices, ShowDomain } from "./choices.ts";

/** Whether a domain is on, off, or on with at least one but not every one of its sources off. */
export type DomainState = "on" | "off" | "mixed";

export function domainState(domain: ShowDomain, hidden: Hidden): DomainState {
  if (hidden.domain.has(domain.name)) return "off";
  const off = domain.sources.filter((s) => hidden.source.has(s.id)).length;
  return off > 0 && off < domain.sources.length ? "mixed" : "on";
}

const every = (group: Group, names: readonly string[], shown: boolean): Change[] => names.map((name) => ({ group, name, shown }));

/** The domain on, and every source under it. */
export const domainOn = (domain: ShowDomain): Change[] => [
  { group: "domain", name: domain.name, shown: true },
  ...every(
    "source",
    domain.sources.map((s) => s.id),
    true,
  ),
];

/** The domain off, its sources' switches as they are. */
export const domainOff = (domain: ShowDomain): Change[] => [{ group: "domain", name: domain.name, shown: false }];

/** Only the source `id`: it and each domain whose points it places on, every other source off. */
export function onlySource(id: string, choices: ShowChoices): Change[] {
  const holders = choices.domains.filter((d) => d.sources.some((s) => s.id === id)).map((d) => d.name);
  return [
    ...every("domain", holders, true),
    ...choices.sources.map((name) => ({ group: "source" as const, name, shown: name === id })),
  ];
}

const structureKinds = (choices: ShowChoices) => choices.families.flatMap((f) => f.kinds.map((k) => k.kind));

/** Only the kind of structure `kind`: it on, every other kind off. */
export const onlyKind = (kind: string, choices: ShowChoices): Change[] =>
  structureKinds(choices).map((name) => ({ group: "structure", name, shown: name === kind }));

/** Every domain, source and kind of value on. */
export const allPoints = (choices: ShowChoices): Change[] => [
  ...every(
    "domain",
    choices.domains.map((d) => d.name),
    true,
  ),
  ...every("source", choices.sources, true),
  ...every("kind", KINDS, true),
];

/** Every domain off, so no point is drawn, and turning a domain on brings its sources back. */
export const nonePoints = (choices: ShowChoices): Change[] =>
  every(
    "domain",
    choices.domains.map((d) => d.name),
    false,
  );

/** Every kind of structure on, or off. */
export const allStructures = (choices: ShowChoices): Change[] => every("structure", structureKinds(choices), true);
export const noneStructures = (choices: ShowChoices): Change[] => every("structure", structureKinds(choices), false);

/** Every group turned off now, on. */
export const everything = (hidden: Hidden): Change[] => GROUPS.flatMap((group) => every(group, [...hidden[group]], true));

/** Whether any group is off. */
export const anyOff = (hidden: Hidden): boolean => GROUPS.some((group) => hidden[group].size > 0);
