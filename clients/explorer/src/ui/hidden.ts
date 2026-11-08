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

/** `hidden` with each of `changes` made in order: the set of a group no change turns on or off
 * kept as it is, so that what depends on it alone is not done again, and `hidden` itself where
 * nothing changes, so that nothing is drawn again. */
export function applied(hidden: Hidden, changes: readonly Change[]): Hidden {
  const next: Record<Group, Set<string> | ReadonlySet<string>> = { ...hidden };
  let changed = false;
  for (const { group, name, shown } of changes) {
    if (next[group].has(name) !== shown) continue;
    const set = next[group] === hidden[group] ? new Set(hidden[group]) : (next[group] as Set<string>);
    if (shown) set.delete(name);
    else set.add(name);
    next[group] = set;
    changed = true;
  }
  return changed ? next : hidden;
}

/** The groups turned off, read from the page's address and kept in it, and the way to turn
 * groups on or off. */
export function useHidden(): [Hidden, Apply] {
  const [hidden, setHidden] = useState<Hidden>(() => hiddenInAddress(location.search));
  useEffect(() => setInAddress({ hide: hideParameter(hidden) }), [hidden]);
  const apply = useCallback<Apply>((changes) => setHidden((h) => applied(h, changes)), []);
  return [hidden, apply];
}
