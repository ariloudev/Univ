// The groups turned off (choose-what-is-drawn): scale domains, sources of the drawn points,
// kinds of value and kinds of structure, kept in the page's address as one `hide` parameter.

import { useCallback, useEffect, useState } from "react";

import { KIND_INDEX } from "../scene/kinds.ts";
import { setInAddress } from "./address.ts";

/** A group: scale domains, sources by id, kinds of value, kinds of structure. */
export type Group = "domain" | "source" | "kind" | "structure";

/** What is turned off, by group. */
export type Hidden = Readonly<Record<Group, ReadonlySet<string>>>;

/** Turns `name` of `group` on where `shown`, else off. */
export type Toggle = (group: Group, name: string, shown: boolean) => void;

const GROUPS: readonly Group[] = ["domain", "source", "kind", "structure"];

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

/** `hidden` with `name` of `group` turned off, or on again where `shown`; the other groups' sets
 * kept as they are, so that what depends on them alone is not done again. */
export function toggled(hidden: Hidden, group: Group, name: string, shown: boolean): Hidden {
  const next = new Set(hidden[group]);
  if (shown) next.delete(name);
  else next.add(name);
  return { ...hidden, [group]: next };
}

/** The groups turned off, read from the page's address and kept in it, and the way to turn one
 * on or off. */
export function useHidden(): [Hidden, Toggle] {
  const [hidden, setHidden] = useState<Hidden>(() => hiddenInAddress(location.search));
  useEffect(() => setInAddress({ hide: hideParameter(hidden) }), [hidden]);
  const toggle = useCallback<Toggle>((group, name, shown) => setHidden((h) => toggled(h, group, name, shown)), []);
  return [hidden, toggle];
}
