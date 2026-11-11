import { useEffect, useMemo, useRef } from "react";

import type { Cosmology, Rotations } from "../api/schemas.ts";
import type { StructureDetail } from "../api/structures.ts";
import type { Domain } from "../catalogue/tiles.ts";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import { choicesOf } from "./show/choices.ts";
import { Atlas, type AtlasPlace, type Selection } from "./Atlas.tsx";
import { type Hidden, useHidden } from "./hidden.ts";
import { useOutlines } from "./outlinesSwitch.ts";
import { useDustMap } from "./useDustMap.ts";
import { useChosen } from "./useChosen.ts";
import { type Structures, flyToStructure, useStructures } from "./useStructures.ts";
import { volumeOf } from "../scene/volume.ts";
import { useRecordContext } from "./useRecordContext.ts";
import { type Target, isChosen, targetOf } from "./letGo.ts";
import { type Inside, type StructureChoice, useStructureChoice } from "./inside.ts";
import { type World, cull, hide, useWorld } from "./useWorld.ts";
import { type AgeCut, logAgeOf, useAgeCut } from "./age.ts";
import type { AgeState } from "./show/AgeChoice.tsx";

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

/** The age choice the Show drawer offers, once `world` is made: the tracers it holds read as
 * each frame is drawn. */
const NO_AGES = { tracers: 0, aged: 0 };
function ageState(world: World | undefined, cut: AgeCut, choose: (cut: AgeCut) => void): AgeState | undefined {
  if (world === undefined) return undefined;
  return { cut, choose, frame: world.frame, held: () => world.catalogue?.agesHeld() ?? NO_AGES };
}

/** Applies what is hidden of the points, the age of the tracers drawn and the level of detail to
 * `world`'s catalogue. */
function usePointSettings(world: World | undefined, domains: readonly Domain[], hidden: Hidden, age: AgeCut, levelOfDetail: boolean): void {
  // Only on a change of what points belong to: a kind of structure turned off counts no point.
  const { domain, source, kind } = hidden;
  useEffect(() => {
    if (world !== undefined) hide(world, { domain, source, kind }, domains);
  }, [world, domains, domain, source, kind]);
  // `domains` is read once the catalogue is made, so that it starts as the choice stands.
  useEffect(() => {
    if (world === undefined) return;
    world.catalogue?.setAgeCut(logAgeOf(age));
    world.controller.invalidate();
  }, [world, domains, age]);
  // `domains` is read once the catalogue is made, so that it starts as the switch stands.
  useEffect(() => {
    world?.catalogue?.setLevelOfDetail(levelOfDetail);
  }, [world, domains, levelOfDetail]);
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
  cosmology: Cosmology | undefined;
  levelOfDetail: boolean;
  place: AtlasPlace;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const structureChoice = useStructureChoice();
  const structureId = structureChoice.structure;
  const { world, domains, rotations, overlay, problem } = useWorld(release, canvas, {
    point: (point) => selection.object(point.id),
    structure: (id) => selection.structureById(id),
    isChosen: (found) => chosenAlready(targetOf(found)),
    letGo: () => selection.close(),
  });
  const { chosen, chosenId, choose } = useChosen(release, world);
  /** Whether `target` is what is chosen, though its record is still being read. */
  const chosenAlready = (target: Target) => isChosen(target, { object: chosenId, structure: structureId });
  const [hidden, apply] = useHidden();
  const [age, chooseAge] = useAgeCut();
  usePointSettings(world, domains, hidden, age, levelOfDetail);
  const outlines = useOutlines();
  const dust = useDustMap(release, world, rotations);
  const structures = useStructures(release, world, rotations, structureId, hidden.structure, outlines[0]);
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
    structureId,
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
    structureFromList: (id) => {
      if (chosenAlready({ structure: id })) selection.close();
      else selection.structureById(id);
    },
    close: () => {
      choose(undefined);
      structureChoice.close();
      // The view no longer turns and zooms about the place of what was let go.
      world?.controller.returnToEarth();
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
      show={{ choices, hidden, onApply: apply, dust, age: ageState(world, age, chooseAge) }}
      outlines={outlines}
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
