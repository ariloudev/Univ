import { useCallback, useRef, useState } from "react";

import { HoverLabel, Marks, StructureNames } from "./SceneOverlay.tsx";
import type { ScenePoint } from "./scenePoint.ts";
import { useChosen } from "./useChosen.ts";
import { SidePanels } from "./SidePanels.tsx";
import { structureView, useStructures } from "./useStructures.ts";
import { useWorld } from "./useWorld.ts";

type Hover = { x: number; y: number; point: ScenePoint } | undefined;

/** The view of `release`: the canvas, its overlay, and the panels around it. */
export function Scene({ release, cosmology }: { release: number; cosmology: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<Hover>();
  const [structureId, setStructureId] = useState<string>();
  const chooseRef = useRef<(id: number | undefined, fly: boolean) => void>(() => undefined);
  const onChoose = useCallback((point: ScenePoint, fly: boolean) => {
    setStructureId(undefined);
    chooseRef.current(point.id, fly);
  }, []);
  const { world, overlay, problem } = useWorld(release, canvas, { onHover: setHover, onChoose });
  const { chosen, choose } = useChosen(release, world);
  chooseRef.current = choose;
  const structures = useStructures(release, world, world?.rotations !== undefined);
  const chooseStructure = (id: string) => {
    choose(undefined, false);
    setStructureId(id);
    const view = structureView(structures, id);
    if (world !== undefined && view !== undefined) world.controller.flyTo(view.centre, view.distance);
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
      <StructureNames drawings={structures.drawings.values()} overlay={overlay} onChoose={chooseStructure} />
      <HoverLabel hover={hover} />
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
          chosen={chosen}
          structureId={structureId}
          onChooseObject={(id) => {
            setStructureId(undefined);
            choose(id, true);
          }}
          onCloseObject={() => choose(undefined, false)}
          onChooseStructure={chooseStructure}
          onCloseStructure={() => setStructureId(undefined)}
        />
      )}
    </div>
  );
}
