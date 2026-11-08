// The groups turned off (choose-what-is-drawn): scale domains, sources of the drawn points,
// kinds of value and kinds of structure, kept in the page's address as one `hide` parameter.

import { useCallback, useEffect, useState } from "react";

import { KIND_INDEX } from "../scene/kinds.ts";
import { setInAddress } from "./address.ts";

/** A group: scale domains, sources by id, kinds of value, kinds of structure. */
export type Group = "domain" | "source" | "kind" | "structure";

/** What is turned off, by group. */
export type Hidden = Readonly<Record<Group, ReadonlySet<string>>>;

/** One group turned on where `shown`, else off. */
export interface Change {
  group: Group;
  name: string;
  shown: boolean;
}

/** Makes `changes`, in order, at once. */
export type Apply = (changes: readonly Change[]) => void;

export const GROUPS: readonly Group[] = ["domain", "source", "kind", "structure"];

const isGroup = (name: string): name is Group => (GROUPS as readonly string[]).includes(name);

/** Nothing turned off: everything drawn. */
export const NOTHING_HIDDEN: Hidden = { domain: new Set(), source: new Set(), kind: new Set(), structure: new Set() };

/** The groups the page's address turns off: `hide=domain:cosmic_web,source:twomrs,…`. Entries
 * that name no group, or no kind of value, are left out. */
export function hiddenInAddress(search: string): Hidden {
  const sets = { domain: new Set<string>(), source: new Set<string>(), kind: new Set<string>(), structure: new Set<string>() };
  for (const entry of (new URLSearchParams(search).get("hide") ?? "").split(",")) {
    const [group = "", name = ""] = entry.split(":", 2);
    if (name === "" || !isGroup(group) || (group === "kind" && !Object.hasOwn(KIND_INDEX, name))) continue;
    sets[group].add(name);
  }
  return sets;
}

/** The `hide` parameter for `hidden`, or none where nothing is hidden. */
export function hideParameter(hidden: Hidden): string | undefined {
  const entries = GROUPS.flatMap((group) => [...hidden[group]].sort().map((name) => `${group}:${name}`));
  return entries.length === 0 ? undefined : entries.join(",");
}

/** `hidden` with each of `changes` made in order; the sets of the groups no change turns on or
 * off kept as they are, so that what depends on them alone is not done again. */
export function applied(hidden: Hidden, changes: readonly Change[]): Hidden {
  const next: Record<Group, ReadonlySet<string>> = { ...hidden };
  for (const group of GROUPS) {
    const own = changes.filter((c) => c.group === group && c.shown === hidden[group].has(c.name));
    if (own.length === 0) continue;
    const set = new Set(hidden[group]);
    for (const c of changes) if (c.group === group) c.shown ? set.delete(c.name) : set.add(c.name);
    next[group] = set;
  }
  return next;
}

/** The groups turned off, read from the page's address and kept in it, and the way to turn
 * groups on or off. */
export function useHidden(): [Hidden, Apply] {
  const [hidden, setHidden] = useState<Hidden>(() => hiddenInAddress(location.search));
  useEffect(() => setInAddress({ hide: hideParameter(hidden) }), [hidden]);
  const apply = useCallback<Apply>((changes) => setHidden((h) => applied(h, changes)), []);
  return [hidden, apply];
}
