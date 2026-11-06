import { useEffect, useMemo, useRef } from "react";

import type { Rotations } from "../api/schemas.ts";
import type { StructureDetail } from "../api/structures.ts";
import type { Domain } from "../catalogue/tiles.ts";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import { choicesOf } from "./ShowPanel.tsx";
import { type Selection, SidePanels } from "./SidePanels.tsx";
import { useHidden } from "./hidden.ts";
import { useChosen } from "./useChosen.ts";
import { type Structures, structureView, useStructures } from "./useStructures.ts";
import { volumeOf } from "../scene/volume.ts";
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
  useEffect(() => {
    if (world !== undefined) cull(world, inside ? volume : undefined);
  }, [world, domains, inside, volume]);
  const flyOnRead = useRef(choice.structure);
  useEffect(() => {
    const view = flyOnRead.current === undefined ? undefined : structureView(structures, flyOnRead.current);
    if (world === undefined || view === undefined) return;
    flyOnRead.current = undefined;
    world.controller.flyTo(view.centre, view.distance);
  }, [world, structures]);
  return { detail, inside: volume === undefined ? undefined : { on: inside, approximate: volume.approximate, set: setInside } };
}

/** The view of `release`: the canvas, its overlay, and the panels around it; its tiles at the
 * detail the view needs, or every one where `levelOfDetail` is off. */
export function Scene({ release, cosmology, levelOfDetail }: { release: number; cosmology: string; levelOfDetail: boolean }) {
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
  const selection: Selection = {
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
      const view = structureView(structures, id);
      if (world !== undefined && view !== undefined) world.controller.flyTo(view.centre, view.distance);
    },
    close: () => {
      choose(undefined);
      structureChoice.close();
    },
  };
  return (
    <div className="relative h-full w-full overflow-hidden">
      <canvas
        ref={canvas}
        className="block h-full w-full cursor-grab touch-none active:cursor-grabbing"
        role="img"
        aria-label={`The placed objects of release ${release} around Earth. Drag to turn, scroll to zoom, drag with the right button or Shift to move; click a point to read it and centre the view on it, double-click to fly to it.`}
      />
      <Marks overlay={overlay} />
      <StructureNames names={structures.names} overlay={overlay} chosen={structureId} />
      {world === undefined ? null : <HoverLabel world={world} />}
      {problem === undefined ? null : (
        <p role="alert" className="absolute inset-x-0 top-4 mx-auto w-fit rounded bg-panel px-3 py-2 text-warn">
          {problem}
        </p>
      )}
      {world === undefined ? null : (
        <SidePanels
          world={world}
          release={release}
          cosmology={cosmology}
          structures={structures}
          selection={selection}
          show={{ choices, hidden, onToggle: toggle }}
        />
      )}
    </div>
  );
}
