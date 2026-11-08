import { useEffect, useState } from "react";

import type { ObjectRecord } from "../api/objects.ts";
import { type SourceDetail, sourceDetail, sourceTitles } from "../api/sources.ts";

/** What the object's panel knows of the release's sources: each one's title, and the records of
 * those whose flags or relations the object carries, for what each flag value means and the form
 * a related object's designation is written in. */
export interface Sources {
  titles: ReadonlyMap<string, string>;
  details: ReadonlyMap<string, SourceDetail>;
}

const NO_SOURCES: Sources = { titles: new Map(), details: new Map() };

/** The sources whose records `record`'s panel reads: those of its flagged entries and of the
 * objects it is related to. */
const detailed = (record: ObjectRecord): string[] => [
  ...new Set([
    ...record.entries.filter((e) => Object.keys(e.flags ?? {}).length > 0).map((e) => e.source),
    ...(record.relations ?? []).map((r) => r.other.source),
  ]),
];

/** The sources of `release` for `record`'s panel, read once each and kept. Until they are read,
 * or where they cannot be, a source is named by its id and a flag by its value alone. */
export function useSources(release: number, record: ObjectRecord | undefined): Sources {
  const [sources, setSources] = useState<Sources>(NO_SOURCES);
  useEffect(() => {
    setSources(NO_SOURCES);
    if (record === undefined) return;
    let live = true;
    void Promise.all([
      sourceTitles(release).catch(() => NO_SOURCES.titles),
      Promise.all(detailed(record).map((id) => sourceDetail(id, release).catch(() => undefined))),
    ]).then(([titles, read]) => {
      if (live) setSources({ titles, details: new Map(read.flatMap((d) => (d === undefined ? [] : [[d.id, d] as const]))) });
    });
    return () => {
      live = false;
    };
  }, [release, record]);
  return sources;
}
