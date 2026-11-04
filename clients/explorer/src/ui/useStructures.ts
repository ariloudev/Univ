// The release's structures (see-the-structures): listed, read, and drawn into the view.

import { useEffect, useState } from "react";

import { type StructureDetail, type StructureSummary, structureOf, structuresOf } from "../api/structures.ts";
import type { Lines } from "../scene/drawing.ts";
import { APPROXIMATE_COLOUR, STRUCTURE_COLOUR } from "../scene/kinds.ts";
import { type StructureDrawing, drawingOf } from "../scene/structures.ts";
import { viewingDistance } from "../scene/controls.ts";
import { type Vec3, norm, sub } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** What the explorer knows of the release's structures. */
export interface Structures {
  list: StructureSummary[];
  details: ReadonlyMap<string, StructureDetail>;
  drawings: ReadonlyMap<string, StructureDrawing>;
  problem: string | undefined;
}

/** Offsets of `points` from `centre`, flat, for the renderer. */
const offsetsFrom = (centre: Vec3, points: readonly Vec3[]) => new Float32Array(points.flatMap((p) => sub(p, centre)));

/** What the renderer draws of `drawing`: its lines and its markers, about a centre. */
export function rendered(drawing: StructureDrawing): { lines: Lines | undefined; markers: { centre: Vec3; offsets: Float32Array } | undefined } {
  const { segments, markers } = drawing.outline;
  const centre = drawing.centre ?? segments[0] ?? markers[0];
  if (centre === undefined) return { lines: undefined, markers: undefined };
  const colour = drawing.approximate ? APPROXIMATE_COLOUR : STRUCTURE_COLOUR;
  return {
    lines: segments.length === 0 ? undefined : { centre, offsets: offsetsFrom(centre, segments), colour },
    markers: markers.length === 0 ? undefined : { centre, offsets: offsetsFrom(centre, markers) },
  };
}

/** Draws `drawing` into `world`'s renderer. */
function draw(world: World, drawing: StructureDrawing): void {
  const { lines, markers } = rendered(drawing);
  world.renderer.setLines(`structure:${drawing.id}`, lines);
  world.renderer.setMarkers(`structure:${drawing.id}`, markers);
}

/** Where to fly to see structure `id` whole: its centre, from two and a half times its size. */
export function structureView(structures: Structures, id: string): { centre: Vec3; distance: number } | undefined {
  const centre = structures.drawings.get(id)?.centre;
  if (centre === undefined) return undefined;
  const bounds = structures.details.get(id)?.bounds;
  const size = bounds === null || bounds === undefined ? undefined : norm(sub(bounds.max, bounds.min));
  return { centre, distance: viewingDistance(norm(centre), size) };
}

async function readAll(release: number, signal: AbortSignal): Promise<{ list: StructureSummary[]; details: StructureDetail[] }> {
  const list = await structuresOf(release, { signal });
  const details = await Promise.all(list.map((s) => structureOf(s.id, release, { signal })));
  return { list, details };
}

/** The structures of `release`, drawn into `world` once its rotations are known. */
export function useStructures(release: number, world: World | undefined, ready: boolean): Structures {
  const [structures, setStructures] = useState<Structures>({ list: [], details: new Map(), drawings: new Map(), problem: undefined });
  useEffect(() => {
    const rotations = world?.rotations;
    if (world === undefined || !ready || rotations === undefined) return;
    const abort = new AbortController();
    readAll(release, abort.signal)
      .then(({ list, details }) => {
        const drawings = new Map(details.map((d) => [d.id, drawingOf(d, rotations)]));
        for (const drawing of drawings.values()) draw(world, drawing);
        world.controller.invalidate();
        setStructures({ list, details: new Map(details.map((d) => [d.id, d])), drawings, problem: undefined });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setStructures((s) => ({ ...s, problem: `The structures could not be read: ${String(error)}` }));
      });
    return () => abort.abort();
  }, [release, world, ready]);
  return structures;
}
