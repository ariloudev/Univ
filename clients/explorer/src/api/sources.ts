// The sources of a release as the explorer reads them (read-a-source): their titles, to name
// each designation, name and catalogue entry by its source; and, as an object's panel needs
// them, what each value of a source's flags means. Each is read once for each release.

import { z } from "zod";

import { type ReadOptions, readJson } from "./client.ts";

const Listing = z.looseObject({ sources: z.array(z.looseObject({ id: z.string(), title: z.string() })) });

const FlagMeanings = z.looseObject({ values: z.record(z.string(), z.string()), cite: z.string() });

const SourceDetail = z.looseObject({
  id: z.string(),
  title: z.string(),
  /** What each value of each flag column means, by column, where the definition gives meanings. */
  flags: z.record(z.string(), FlagMeanings).optional(),
});
export type SourceDetail = z.infer<typeof SourceDetail>;

const titles = new Map<number, Promise<ReadonlyMap<string, string>>>();
const details = new Map<string, Promise<SourceDetail>>();

/** Keeps `promise` under `key` in `cache` until it fails. */
function kept<K, V>(cache: Map<K, Promise<V>>, key: K, read: () => Promise<V>): Promise<V> {
  const found = cache.get(key) ?? read();
  if (!cache.has(key)) {
    cache.set(key, found);
    found.catch(() => cache.delete(key));
  }
  return found;
}

/** Each source of `release`, its title by its id. */
export function sourceTitles(release: number, options: ReadOptions = {}): Promise<ReadonlyMap<string, string>> {
  return kept(titles, release, async () => {
    const listing = await readJson("/v1/sources", Listing, { ...options, release });
    return new Map(listing.sources.map((s) => [s.id, s.title]));
  });
}

/** The record of source `id` in `release`. */
export function sourceDetail(id: string, release: number, options: ReadOptions = {}): Promise<SourceDetail> {
  return kept(details, `${release}:${id}`, () => readJson(`/v1/sources/${encodeURIComponent(id)}`, SourceDetail, { ...options, release }));
}

/** Forgets every source read, as a test begins. */
export const forgetSources = (): void => {
  titles.clear();
  details.clear();
};
