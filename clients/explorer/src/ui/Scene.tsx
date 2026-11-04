import { useRef, useState } from "react";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import { type Selection, SidePanels } from "./SidePanels.tsx";
import { useChosen } from "./useChosen.ts";
import { structureView, useStructures } from "./useStructures.ts";
import { useWorld } from "./useWorld.ts";

/** The view of `release`: the canvas, its overlay, and the panels around it. */
export function Scene({ release, cosmology }: { release: number; cosmology: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [structureId, setStructureId] = useState<string>();
  const { world, rotations, overlay, problem } = useWorld(release, canvas, {
    point: (point) => selection.object(point.id),
    structure: (id) => selection.structureById(id),
  });
  const { chosen, choose } = useChosen(release, world);
  const structures = useStructures(release, world, rotations);
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
        aria-label={`The placed objects of release ${release} around Earth. Drag to turn, scroll to zoom, drag with the right button or Shift to move; click a point to read it, double-click to fly to it.`}
      />
      <Marks overlay={overlay} />
      <StructureNames names={world?.names ?? []} overlay={overlay} />
      {world === undefined ? null : <HoverLabel world={world} />}
      {problem === undefined ? null : (
        <p role="alert" className="absolute inset-x-0 top-4 mx-auto w-fit rounded bg-panel px-3 py-2 text-warn">
          {problem}
        </p>
      )}
      {world === undefined ? null : (
        <SidePanels world={world} release={release} cosmology={cosmology} structures={structures} selection={selection} />
      )}
    </div>
  );
}
