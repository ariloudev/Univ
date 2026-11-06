import { useEffect, useMemo, useRef, useState } from "react";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import { choicesOf } from "./ShowPanel.tsx";
import { type Selection, SidePanels } from "./SidePanels.tsx";
import { useHidden } from "./hidden.ts";
import { useChosen } from "./useChosen.ts";
import { structureView, useStructures } from "./useStructures.ts";
import { hide, useWorld } from "./useWorld.ts";

/** The view of `release`: the canvas, its overlay, and the panels around it; its tiles at the
 * detail the view needs, or every one where `levelOfDetail` is off. */
export function Scene({ release, cosmology, levelOfDetail }: { release: number; cosmology: string; levelOfDetail: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [structureId, setStructureId] = useState<string>();
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
  const choices = useMemo(() => choicesOf(domains, structures.list), [domains, structures.list]);
  const selection: Selection = {
    chosen,
    structure: structureId === undefined ? undefined : structures.details.get(structureId),
    approximate: structureId !== undefined && structures.drawings.get(structureId)?.approximate === true,
    object: (id, options) => {
      setStructureId(undefined);
      choose(id, options);
    },
    structureById: (id) => {
      choose(undefined);
      setStructureId(id);
      const view = structureView(structures, id);
      if (world !== undefined && view !== undefined) world.controller.flyTo(view.centre, view.distance);
    },
    close: () => {
      choose(undefined);
      setStructureId(undefined);
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
