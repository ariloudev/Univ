// The release's structures (see-the-structures): listed, read, and drawn into the view.

import { useEffect, useState } from "react";

import type { Rotations } from "../api/schemas.ts";
import { type StructureDetail, type StructureSummary, structureOf, structuresOf } from "../api/structures.ts";
import { viewingDistance } from "../scene/controls.ts";
import type { Lines } from "../scene/drawing.ts";
import { APPROXIMATE_COLOUR, CHOSEN_COLOUR, STRUCTURE_COLOUR } from "../scene/kinds.ts";
import { type StructureDrawing, drawingOf } from "../scene/structures.ts";
import { type Vec3, norm, sub } from "../scene/vec.ts";
import type { World } from "./useWorld.ts";

/** What the explorer knows of the release's structures. */
export interface Structures {
  list: StructureSummary[];
  details: ReadonlyMap<string, StructureDetail>;
  drawings: ReadonlyMap<string, StructureDrawing>;
  problem: string | undefined;
}

const NONE: Structures = { list: [], details: new Map(), drawings: new Map(), problem: undefined };

/** Structure records read at once. */
const PARALLEL = 6;

/** Offsets of `points` from `centre`, flat, for the renderer. */
const offsetsFrom = (centre: Vec3, points: readonly Vec3[]) => new Float32Array(points.flatMap((p) => sub(p, centre)));

/** What the renderer draws of `drawing`: its lines and its markers, about a centre, in the
 * chosen colour where it is the chosen structure. */
export function rendered(drawing: StructureDrawing, chosen = false): { lines: Lines | undefined; markers: { centre: Vec3; offsets: Float32Array } | undefined } {
  const { segments, markers } = drawing.outline;
  const centre = drawing.centre ?? segments[0] ?? markers[0];
  if (centre === undefined) return { lines: undefined, markers: undefined };
  const colour = chosen ? CHOSEN_COLOUR : drawing.approximate ? APPROXIMATE_COLOUR : STRUCTURE_COLOUR;
  return {
    lines: segments.length === 0 ? undefined : { centre, offsets: offsetsFrom(centre, segments), colour },
    markers: markers.length === 0 ? undefined : { centre, offsets: offsetsFrom(centre, markers) },
  };
}

/** Where to fly to see structure `id` whole: its centre, from two and a half times its size. */
export function structureView(structures: Structures, id: string): { centre: Vec3; distance: number } | undefined {
  const drawing = structures.drawings.get(id);
  if (drawing?.centre === undefined) return undefined;
  return { centre: drawing.centre, distance: viewingDistance(norm(drawing.centre), drawing.size) };
}

/** `run` on each of `items`, at most `limit` at once, each settled whatever the others do. */
export async function settled<T, R>(items: readonly T[], limit: number, run: (item: T) => Promise<R>): Promise<PromiseSettledResult<R>[]> {
  const results: PromiseSettledResult<R>[] = new Array(items.length);
  let next = 0;
  const worker = async () => {
    for (let i = next++; i < items.length; i = next++) {
      try {
        results[i] = { status: "fulfilled", value: await run(items[i] as T) };
      } catch (reason) {
        results[i] = { status: "rejected", reason };
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return results;
}

/** Reads every structure of `release`, those whose records could not be read listed but not drawn. */
async function readAll(release: number, rotations: Rotations, signal: AbortSignal): Promise<Structures> {
  const list = await structuresOf(release, { signal });
  const results = await settled(list, PARALLEL, (s) => structureOf(s.id, release, { signal }));
  const details = results.flatMap((r) => (r.status === "fulfilled" ? [r.value] : []));
  const missing = list.length - details.length;
  return {
    list,
    details: new Map(details.map((d) => [d.id, d])),
    drawings: new Map(details.map((d) => [d.id, drawingOf(d, rotations)])),
    problem: missing === 0 ? undefined : `${missing} structure records could not be read and are not drawn.`,
  };
}

/** Draws `drawing` into `world`, in the chosen colour where `chosen`. */
function draw(world: World, drawing: StructureDrawing, chosen: boolean): void {
  const { lines, markers } = rendered(drawing, chosen);
  world.renderer.setLines(`structure:${drawing.id}`, lines);
  world.renderer.setMarkers(`structure:${drawing.id}`, markers, chosen);
}

/** Draws every structure of `structures` into `world`, and hands their names to the overlay. */
function drawAll(world: World, structures: Structures): void {
  for (const drawing of structures.drawings.values()) draw(world, drawing, false);
  world.names = [...structures.drawings.values()].flatMap((d) =>
    d.centre === undefined ? [] : [{ id: d.id, centre: d.centre, text: d.approximate ? `${d.name} (approximate)` : d.name }],
  );
  world.controller.invalidate();
}

/** The structures of `release`, drawn into `world` once the rotations of its frames are read,
 * the one `chosen` in the chosen colour. */
export function useStructures(release: number, world: World | undefined, rotations: Rotations | undefined, chosen?: string): Structures {
  const [structures, setStructures] = useState<Structures>(NONE);
  useEffect(() => {
    const drawing = chosen === undefined ? undefined : structures.drawings.get(chosen);
    if (world === undefined || drawing === undefined) return;
    draw(world, drawing, true);
    world.controller.invalidate();
    // Chosen no more: drawn again in its own colour.
    return () => {
      draw(world, drawing, false);
      world.controller.invalidate();
    };
  }, [world, structures, chosen]);
  useEffect(() => {
    if (world === undefined || rotations === undefined) return;
    const abort = new AbortController();
    readAll(release, rotations, abort.signal)
      .then((read) => {
        drawAll(world, read);
        setStructures(read);
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setStructures({ ...NONE, problem: `The structures could not be read: ${String(error)}` });
      });
    return () => abort.abort();
  }, [release, world, rotations]);
  return structures;
}
