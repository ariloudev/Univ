import { useMemo } from "react";

import { search } from "../api/objects.ts";
import type { Rotations } from "../api/schemas.ts";
import { inside, volumeOf } from "../scene/volume.ts";
import type { RecordContext } from "./ObjectPanel.tsx";
import type { ChooseOptions, ChosenObject } from "./useChosen.ts";
import type { Structures } from "./useStructures.ts";
import { useSources } from "./useSources.ts";

/** What the object's panel reads besides the record of `chosen` in `release`: the release's
 * sources, the rotation to galactic axes, the structures whose volume holds a place, and the ways
 * to choose a structure, or an object by the designation its source gives it. */
export function useRecordContext(
  release: number,
  chosen: ChosenObject,
  rotations: Rotations | undefined,
  structures: Structures,
  choose: { structure: (id: string) => void; object: (id: number, options: ChooseOptions) => void },
): RecordContext {
  const sources = useSources(release, chosen.kind === "ready" ? chosen.record : undefined);
  // Each structure's volume, once the structures and the release's rotations are read.
  const volumes = useMemo(
    () =>
      rotations === undefined
        ? []
        : [...structures.details.values()].flatMap((d) => {
            const volume = volumeOf(d, rotations);
            return volume === undefined ? [] : [{ id: d.id, name: d.name, volume }];
          }),
    [structures.details, rotations],
  );
  return {
    sources,
    toGalactic: rotations?.to_galactic,
    holding: (place) => volumes.filter((v) => inside(v.volume, ...place)).map(({ id, name }) => ({ id, name })),
    onChooseStructure: choose.structure,
    onChooseDesignation: (source, identifier) => {
      search(`${source}:${identifier}`, { release })
        .then((found) => {
          if (found.kind === "object") choose.object(found.record.id, { record: found.record, fly: true });
        })
        .catch(() => undefined);
    },
  };
}
