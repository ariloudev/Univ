// The sources of a release as the explorer reads them (read-a-source): their titles, to name
// each designation, name and catalogue entry by its source; and, as an object's panel needs
// them, what each value of a source's flags means. Each is read once for each release.

import { z } from "zod";

import { type ReadOptions, cacheOf, kept, readJson } from "./client.ts";

const Listing = z.looseObject({ sources: z.array(z.looseObject({ id: z.string(), title: z.string() })) });

const FlagMeanings = z.looseObject({ values: z.record(z.string(), z.string()), cite: z.string() });

const SourceDetail = z.looseObject({
  id: z.string(),
  title: z.string(),
  /** The form its catalogue writes a designation in, such as `PGC {identifier}`. */
  designation: z.looseObject({ form: z.string() }).nullable().optional(),
  /** What each value of each flag column means, by column, where the definition gives meanings. */
  flags: z.record(z.string(), FlagMeanings).optional(),
});
export type SourceDetail = z.infer<typeof SourceDetail>;

const titles = cacheOf<number, ReadonlyMap<string, string>>();
const details = cacheOf<string, SourceDetail>();

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
