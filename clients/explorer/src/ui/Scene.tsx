import { useRef } from "react";

import { Panels } from "./Panels.tsx";
import { useWorld } from "./useWorld.ts";

/** The view of `release`: the canvas, Earth and the ring's label over it, and the panels. */
export function Scene({ release, cosmology }: { release: number; cosmology: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { world, overlay, problem } = useWorld(release, canvas);
  return (
    <div className="relative h-full w-full overflow-hidden">
      <canvas
        ref={canvas}
        className="block h-full w-full cursor-grab touch-none active:cursor-grabbing"
        role="img"
        aria-label={`The placed objects of release ${release} around Earth. Drag to turn, scroll to zoom, drag with the right button or Shift to move.`}
      />
      <div
        ref={overlay.earth}
        className="pointer-events-none absolute top-0 left-0"
        title="The Solar System barycentre, within about 1 AU of Earth"
      >
        <div className="-translate-x-1/2 -translate-y-1/2 text-xs text-accent">
          <span className="mx-auto block h-2 w-2 rounded-full bg-accent" />
          <span className="mt-1 block whitespace-nowrap">Earth</span>
        </div>
      </div>
      <div ref={overlay.ring} className="pointer-events-none absolute top-0 left-0 text-xs text-dim">
        <span ref={overlay.ringLabel} className="ml-1 whitespace-nowrap" />
      </div>
      {problem === undefined ? null : (
        <p role="alert" className="absolute inset-x-0 top-4 mx-auto w-fit rounded bg-panel px-3 py-2 text-warn">
          {problem}
        </p>
      )}
      {world === undefined ? null : (
        <Panels controller={world.controller} catalogue={world.catalogue} release={release} cosmology={cosmology} />
      )}
    </div>
  );
}
