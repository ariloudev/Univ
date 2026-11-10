import { useCallback, useMemo } from "react";

import { type ObjectRecord, search } from "../api/objects.ts";
import type { Rotations } from "../api/schemas.ts";
import { holds, volumeOf } from "../scene/volume.ts";
import type { Vec3 } from "../scene/vec.ts";
import type { RecordContext } from "./ObjectPanel.tsx";
import type { ChooseOptions } from "./useChosen.ts";
import type { Structures } from "./useStructures.ts";

/** What the object's panel reads besides the record in `release`: the rotation to galactic axes,
 * the structures whose volume holds a place, and the ways to choose a structure, or an object by
 * the designation its source gives it. */
export function useRecordContext(
  release: number,
  rotations: Rotations | undefined,
  structures: Structures,
  choose: { structure: (id: string) => void; object: (id: number, options: ChooseOptions) => void },
): RecordContext {
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
  const holding = useCallback((place: Vec3) => volumes.filter((v) => holds(v.volume, place)).map(({ id, name }) => ({ id, name })), [volumes]);
  return {
    release,
    toGalactic: rotations?.to_galactic,
    holding,
    onChooseStructure: choose.structure,
    find: (source, identifier) => search(`${source}:${identifier}`, { release }),
    onChooseObject: (id: number, record?: ObjectRecord) => {
      choose.object(id, { record, fly: true });
    },
  };
}
