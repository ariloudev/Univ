import { useEffect, useState } from "react";

import type { ObjectRecord } from "../api/objects.ts";
import { type SourceDetail, sourceDetail, sourceTitles } from "../api/sources.ts";

/** What the object's panel knows of the release's sources: each one's title, and the records of
 * those whose flags the chosen object carries, for what each value means. */
export interface Sources {
  titles: ReadonlyMap<string, string>;
  details: ReadonlyMap<string, SourceDetail>;
}

export const NO_SOURCES: Sources = { titles: new Map(), details: new Map() };

/** The sources of `release` once `record` is shown, with the records of those whose flags it
 * carries. A source
 * that cannot be read is named by its id, and its flags by their values alone. */
export function useSources(release: number, record: ObjectRecord | undefined): Sources {
  const [titles, setTitles] = useState<ReadonlyMap<string, string>>(NO_SOURCES.titles);
  const [details, setDetails] = useState<ReadonlyMap<string, SourceDetail>>(NO_SOURCES.details);
  // The titles are read once a record is shown, not as the view opens.
  const shown = record !== undefined;
  useEffect(() => {
    if (!shown) return;
    let live = true;
    sourceTitles(release)
      .then((read) => {
        if (live) setTitles(read);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [release, shown]);
  const flagged = [...new Set((record?.entries ?? []).filter((e) => Object.keys(e.flags ?? {}).length > 0).map((e) => e.source))].join(",");
  useEffect(() => {
    if (flagged === "") return;
    let live = true;
    Promise.all(flagged.split(",").map((id) => sourceDetail(id, release).catch(() => undefined)))
      .then((read) => {
        if (live) setDetails(new Map(read.flatMap((d) => (d === undefined ? [] : [[d.id, d] as const]))));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [release, flagged]);
  return { titles, details };
}
