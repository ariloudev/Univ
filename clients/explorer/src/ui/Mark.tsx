import type { KindOfValue } from "univ-tile-decoder";

import { KIND_MARKS, POINT_CSS } from "../scene/kinds.ts";

/** The mark of the kind of value `kind`, as the key shows it; nothing for a kind it has none for. */
export function Mark({ kind }: { kind: string | undefined }) {
  const mark = kind === undefined ? undefined : KIND_MARKS[kind as KindOfValue];
  if (mark === undefined) return null;
  return (
    <span role="img" aria-label={kind} title={kind} data-kind={kind} className="mr-1.5 inline-block w-3 text-center" style={{ color: POINT_CSS }}>
      {mark}
    </span>
  );
}
