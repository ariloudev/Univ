import { useEffect, useMemo, useRef } from "react";

import type { Rotations } from "../api/schemas.ts";
import type { StructureDetail } from "../api/structures.ts";
import type { Domain } from "../catalogue/tiles.ts";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import { choicesOf } from "./ShowPanel.tsx";
import { Atlas, type AtlasPlace, type Selection } from "./Atlas.tsx";
import { useHidden } from "./hidden.ts";
import { useChosen } from "./useChosen.ts";
import { type Structures, flyToStructure, useStructures } from "./useStructures.ts";
import { volumeOf } from "../scene/volume.ts";
import { useRecordContext } from "./useRecordContext.ts";
import { type StructureChoice, useStructureChoice } from "./inside.ts";
import type { Inside } from "./StructuresPanel.tsx";
import { type World, cull, hide, useWorld } from "./useWorld.ts";

/** The structure `choice` names, and the cull to its volume, where it has one
 * (see-the-structures): `world` culled to it while the choice says, and flown to once it is read
 * where the address chose it. `domains` is read once the catalogue is made, so that it starts
 * culled as the view stands. */
function useCulledStructure(
  world: World | undefined,
  domains: readonly Domain[],
  rotations: Rotations | undefined,
  structures: Structures,
  choice: StructureChoice,
): { detail: StructureDetail | undefined; inside: Inside | undefined } {
  const detail = choice.structure === undefined ? undefined : structures.details.get(choice.structure);
  const volume = useMemo(() => (detail === undefined || rotations === undefined ? undefined : volumeOf(detail, rotations)), [detail, rotations]);
  const { inside, setInside } = choice;
  // The cull an address asks for waits, culled to nothing, until the structures are read; a
  // structure with no volume turns it off.
  const read = rotations !== undefined && (structures.list.length > 0 || structures.problem !== undefined);
  useEffect(() => {
    if (world === undefined || (inside && !read)) return;
    if (inside && volume === undefined) setInside(false);
    cull(world, inside ? volume : undefined);
  }, [world, domains, inside, volume, read, setInside]);
  // Flown to once read, unless another structure, or none, is chosen first.
  const flyOnRead = useRef(choice.structure);
  useEffect(() => {
    if (flyOnRead.current !== choice.structure) flyOnRead.current = undefined;
    if (world !== undefined && flyOnRead.current !== undefined && flyToStructure(world, structures, flyOnRead.current)) flyOnRead.current = undefined;
  }, [world, structures, choice.structure]);
  return { detail, inside: volume === undefined ? undefined : { on: inside, approximate: volume.approximate, set: setInside } };
}

/** The view of `release`: the canvas, its overlay, and the rail, margins and bottom bar about it,
 * with its parts of the top bar, all in `place`; its tiles at the detail the view needs, or every one where `levelOfDetail` is
 * off. */
export function Scene({
  release,
  cosmology,
  levelOfDetail,
  place,
}: {
  release: number;
  cosmology: string;
  levelOfDetail: boolean;
  place: AtlasPlace;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const structureChoice = useStructureChoice();
  const structureId = structureChoice.structure;
  const { world, domains, rotations, overlay, problem } = useWorld(release, canvas, {
    point: (point) => selection.object(point.id),
    structure: (id) => selection.structureById(id),
  });
  const { chosen, choose } = useChosen(release, world);
  const [hidden, toggle] = useHidden();
  // Only on a change of what points belong to: a kind of structure turned off counts no point.
  const { domain, source, kind } = hidden;
  useEffect(() => {
    if (world !== undefined) hide(world, { domain, source, kind }, domains);
  }, [world, domains, domain, source, kind]);
  // `domains` is read once the catalogue is made, so that it starts as the switch stands.
  useEffect(() => {
    world?.catalogue?.setLevelOfDetail(levelOfDetail);
  }, [world, domains, levelOfDetail]);
  const structures = useStructures(release, world, rotations, structureId, hidden.structure);
  const { detail, inside } = useCulledStructure(world, domains, rotations, structures, structureChoice);
  const choices = useMemo(() => choicesOf(domains, structures.list), [domains, structures.list]);
  const context = useRecordContext(release, rotations, structures, {
    structure: (id) => selection.structureById(id),
    object: (id, options) => selection.object(id, options),
  });
  const selection: Selection = {
    context,
    chosen,
    structure: detail,
    inside,
    approximate: structureId !== undefined && structures.drawings.get(structureId)?.approximate === true,
    object: (id, options) => {
      structureChoice.close();
      choose(id, options);
    },
    structureById: (id) => {
      choose(undefined);
      structureChoice.choose(id);
      if (world !== undefined) flyToStructure(world, structures, id);
    },
    close: () => {
      choose(undefined);
      structureChoice.close();
    },
  };
  return (
    <Atlas
      world={world}
      release={release}
      cosmology={cosmology}
      place={place}
      structures={structures}
      selection={selection}
      show={{ choices, hidden, onToggle: toggle }}
    >
      <div className="absolute inset-0 overflow-hidden">
        <canvas
          ref={canvas}
          className="block h-full w-full cursor-grab touch-none active:cursor-grabbing"
          role="img"
          aria-label={`The placed objects of release ${release} around Earth. Drag to turn, scroll to zoom, drag with the right button or Shift to move; click a point to read it and centre the view on it, double-click to fly to it.`}
        />
        <Marks overlay={overlay} />
        <StructureNames names={structures.names} overlay={overlay} chosen={structureId} />
        {world === undefined ? null : <HoverLabel world={world} release={release} />}
        {problem === undefined ? null : (
          <p role="alert" className="absolute inset-x-0 top-12 mx-auto w-fit rounded bg-panel px-3 py-2 text-warn">
            {problem}
          </p>
        )}
      </div>
    </Atlas>
  );
}
