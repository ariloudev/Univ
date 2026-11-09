// The release's structures (see-the-structures): listed, read, and drawn into the view.

import { useEffect, useMemo, useState } from "react";

import type { Rotations } from "../api/schemas.ts";
import { type StructureDetail, type StructureSummary, structureOf, structuresOf } from "../api/structures.ts";
import { viewingDistance } from "../scene/controls.ts";
import type { Lines, Rgba } from "../scene/drawing.ts";
import { FLOW_LINE_WIDTH, FLOW_PIXELS, structureColour } from "../scene/kinds.ts";
import { FLOW_REACH } from "../scene/outline.ts";
import { type StructureDrawing, drawingOf } from "../scene/structures.ts";
import { ORIGIN, type Vec3, norm, sub } from "../scene/vec.ts";
import type { NameLabel } from "./overlay.ts";
import type { World } from "./useWorld.ts";

/** What the explorer knows of the release's structures. */
export interface Structures {
  list: StructureSummary[];
  details: ReadonlyMap<string, StructureDetail>;
  drawings: ReadonlyMap<string, StructureDrawing>;
  problem: string | undefined;
}

const NONE: Structures = { list: [], details: new Map(), drawings: new Map(), problem: undefined };

/** The structures, and those of them drawn: of no kind turned off (choose-what-is-drawn). */
export interface ShownStructures extends Structures {
  /** The structures drawn, listed and named over the view. */
  shown: StructureSummary[];
  /** The names of those drawn, to place over the view. */
  names: NameLabel[];
}

/** Structure records read at once. */
const PARALLEL = 6;

/** Offsets of `points` from `centre`, flat, for the renderer. */
const offsetsFrom = (centre: Vec3, points: readonly Vec3[]) => new Float32Array(points.flatMap((p) => sub(p, centre)));

/** What the renderer draws of `drawing`: its lines and its markers, about a centre. */
export function rendered(drawing: StructureDrawing): { lines: Lines | undefined; markers: { centre: Vec3; offsets: Float32Array; colour: Rgba } | undefined } {
  const { segments, markers } = drawing.outline;
  const centre = drawing.centre ?? segments[0] ?? markers[0];
  if (centre === undefined) return { lines: undefined, markers: undefined };
  const colour = structureColour(drawing.kind, drawing.approximate);
  // The symbol is about the structure's own centre, which it has wherever it has one.
  const symbol = drawing.symbol === undefined ? undefined : { offsets: offsetsFrom(ORIGIN, drawing.symbol), pixels: FLOW_PIXELS, maxWidth: FLOW_LINE_WIDTH };
  return {
    lines: segments.length === 0 && symbol === undefined ? undefined : { centre, offsets: offsetsFrom(centre, segments), colour, symbol },
    markers: markers.length === 0 ? undefined : { centre, offsets: offsetsFrom(centre, markers), colour: structureColour(drawing.kind, false) },
  };
}

/** Where to fly to see structure `id` whole: its centre, from two and a half times its size. */
export function structureView(structures: Structures, id: string): { centre: Vec3; distance: number } | undefined {
  const drawing = structures.drawings.get(id);
  if (drawing?.centre === undefined) return undefined;
  return { centre: drawing.centre, distance: viewingDistance(norm(drawing.centre), drawing.size) };
}

/** Flies `world`'s view to the structure `id` of `structures`: whether it is read and placed, and
 * so flown to. */
export function flyToStructure(world: World, structures: Structures, id: string): boolean {
  const view = structureView(structures, id);
  if (view !== undefined) world.controller.flyTo(view.centre, view.distance);
  return view !== undefined;
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

/** The renderer's name for structure `id`'s outline and markers. */
const drawnAs = (id: string) => `structure:${id}`;

/** Draws every structure of `structures` into `world`. */
function drawAll(world: World, structures: Structures): void {
  for (const drawing of structures.drawings.values()) {
    const { lines, markers } = rendered(drawing);
    world.renderer.setLines(drawnAs(drawing.id), lines);
    world.renderer.setMarkers(drawnAs(drawing.id), markers);
  }
}

/** How far a flow symbol's arrows reach from its centre, device pixels: its name goes below. */
export const SYMBOL_REACH = FLOW_REACH * FLOW_PIXELS;

/** The names of the structures drawn of `shown`, to place over the view. */
export const namesOf = (structures: Structures, shown: ReadonlySet<string>): NameLabel[] =>
  [...structures.drawings.values()].flatMap((d) =>
    d.centre === undefined || !shown.has(d.id)
      ? []
      : [{ id: d.id, centre: d.centre, text: d.approximate ? `${d.name} (approximate)` : d.name, below: d.symbol === undefined ? undefined : SYMBOL_REACH }],
  );

/** The renderer's names of the outlines not drawn (see-the-structures, choose-what-is-drawn): of
 * each structure drawn but not `named`, its kind turned off, and, while the `outlines` are off,
 * of every other but the `chosen` one, whose name stays in the view all the same. */
export const hiddenOutlines = (drawn: Iterable<string>, named: ReadonlySet<string>, chosen: string | undefined, outlines: boolean): Set<string> =>
  new Set([...drawn].filter((id) => !named.has(id) || (!outlines && id !== chosen)).map(drawnAs));

/** The structures of `release`, drawn into `world` once the rotations of its frames are read,
 * the one `chosen` in the chosen colour, none of a kind in `hiddenKinds`, and, while `outlines`
 * is false, no outline but the chosen one's, though every name. */
export function useStructures(
  release: number,
  world: World | undefined,
  rotations: Rotations | undefined,
  chosen: string | undefined,
  hiddenKinds: ReadonlySet<string>,
  outlines = true,
): ShownStructures {
  const [structures, setStructures] = useState<Structures>(NONE);
  // The names are worked out as the page renders, so that their elements are there for the
  // frame that places them.
  const shown = useMemo(() => {
    const list = structures.list.filter((s) => !hiddenKinds.has(s.kind));
    return { ...structures, shown: list, names: namesOf(structures, new Set(list.map((s) => s.id))) };
  }, [structures, hiddenKinds]);
  useEffect(() => {
    if (world === undefined) return;
    world.renderer.hideNames(hiddenOutlines(shown.drawings.keys(), new Set(shown.shown.map((s) => s.id)), chosen, outlines));
    world.names = shown.names;
    world.controller.invalidate();
  }, [world, shown, chosen, outlines]);
  // After the render that holds them: a frame drawn before React has added the names' elements
  // would place none, and a still view draws no other.
  useEffect(() => {
    if (world === undefined) return;
    world.renderer.choose(chosen === undefined ? undefined : drawnAs(chosen));
    world.controller.invalidate();
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
  return shown;
}
