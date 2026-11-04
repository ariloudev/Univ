// Starts the scene for a release on a canvas: the renderer, the view's controller, the
// catalogue of tiles and the mouse, and keeps the overlay (Earth, the ring's label, the chosen
// object's mark, the structures' names) in place.

import { type RefObject, useEffect, useRef, useState } from "react";

import type { Rotations } from "../api/schemas.ts";
import { Catalogue } from "../catalogue/catalogue.ts";
import { type Batch, type Domain, domainsOf } from "../catalogue/tiles.ts";
import { Controller, type Surroundings } from "../scene/controller.ts";
import { bindMouse } from "../scene/input.ts";
import { type Pickable, pick } from "../scene/pick.ts";
import { NoWebGL, Renderer } from "../scene/renderer.ts";
import type { Vec3 } from "../scene/vec.ts";
import { type Overlay, placeOverlay } from "./overlay.ts";
import { type ScenePoint, pointOf } from "./scenePoint.ts";

export interface World {
  controller: Controller;
  renderer: Renderer;
  catalogue: Catalogue | undefined;
  domains: ReadonlyMap<string, Domain>;
  rotations: Rotations | undefined;
  /** Every batch drawn, to pick from. */
  pickable: (Pickable & { batch: Batch })[];
  /** Where the chosen object is marked. */
  chosen: Vec3 | undefined;
}

/** What the scene tells the page of the mouse. */
export interface SceneEvents {
  /** The point under the pointer, at `x`, `y`, or none. */
  onHover: (hover: { x: number; y: number; point: ScenePoint } | undefined) => void;
  /** A point chosen: clicked, or double-clicked to fly to it. */
  onChoose: (point: ScenePoint, fly: boolean) => void;
}

function surroundings(element: HTMLElement): Surroundings {
  return {
    size: () => ({ width: element.clientWidth, height: element.clientHeight, ratio: devicePixelRatio }),
    now: () => performance.now(),
    reducedMotion: () => matchMedia("(prefers-reduced-motion: reduce)").matches,
  };
}

/** Reads the release's domains and starts fetching its tiles into `world`. */
async function startCatalogue(release: number, world: World, signal: AbortSignal): Promise<void> {
  const { controller, renderer } = world;
  const domains = await domainsOf(release, { signal });
  const first = domains[0];
  if (first === undefined) return;
  world.domains = new Map(domains.map((d) => [d.name, d]));
  world.rotations = first.manifest.rotations;
  controller.setRotations(first.manifest.rotations);
  const events = {
    onBatch: (batch: Batch) => {
      renderer.addPoints({ centre: batch.tile.centre, offsets: batch.offsets, kinds: batch.kinds });
      world.pickable.push({ centre: batch.tile.centre, radius: batch.tile.edge * 0.87, offsets: batch.offsets, batch });
      controller.invalidate();
    },
    onChange: () => controller.invalidate(),
  };
  const catalogue = new Catalogue(release, domains, events, { signal });
  world.catalogue = catalogue;
  catalogue.update(controller.eye);
}

/** The point of `world` under the pointer at `x`, `y` on `element`, if any. */
function pointAt(world: World, element: HTMLElement, x: number, y: number): ScenePoint | undefined {
  const { controller } = world;
  const pointer = { width: element.clientWidth, height: element.clientHeight, x, y };
  const picked = pick(world.pickable, controller.view, controller.toFrame, pointer);
  const entry = picked === undefined ? undefined : world.pickable[picked.batch];
  return picked === undefined || entry === undefined ? undefined : pointOf(world.domains, entry.batch, picked.index);
}

/** Binds the mouse and the canvas's size to the world, until the returned function. */
function bindView(element: HTMLCanvasElement, world: World, events: RefObject<SceneEvents>): () => void {
  const { controller } = world;
  let hovering: number | undefined;
  const unbind = bindMouse(element, {
    turn: (dx, dy) => controller.turn(dx, dy),
    pan: (dx, dy) => controller.pan(dx, dy),
    zoom: (steps) => controller.zoom(steps),
    click: (x, y) => {
      const point = pointAt(world, element, x, y);
      if (point !== undefined) events.current.onChoose(point, false);
    },
    doubleClick: (x, y) => {
      const point = pointAt(world, element, x, y);
      if (point !== undefined) events.current.onChoose(point, true);
    },
    hover: (x, y) => {
      if (hovering !== undefined) cancelAnimationFrame(hovering);
      hovering = requestAnimationFrame(() => {
        const point = pointAt(world, element, x, y);
        events.current.onHover(point === undefined ? undefined : { x, y, point });
      });
    },
  });
  const resize = new ResizeObserver(() => {
    element.width = Math.round(element.clientWidth * devicePixelRatio);
    element.height = Math.round(element.clientHeight * devicePixelRatio);
    controller.invalidate();
  });
  resize.observe(element);
  return () => {
    unbind();
    resize.disconnect();
    if (hovering !== undefined) cancelAnimationFrame(hovering);
  };
}

/** The scene of `release` on `canvas`: its world, a count that moves on each frame drawn (for
 * the panels), the overlay's elements, and the problem that stopped it, if any. */
export function useWorld(
  release: number,
  canvas: RefObject<HTMLCanvasElement | null>,
  events: SceneEvents,
): { world: World | undefined; version: number; overlay: Overlay; problem: string | undefined } {
  const [version, setVersion] = useState(0);
  const [problem, setProblem] = useState<string>();
  const [world, setWorld] = useState<World>();
  const overlay: Overlay = { earth: useRef(null), ring: useRef(null), ringLabel: useRef(null), chosen: useRef(null), labels: useRef(new Map()) };
  const overlayRef = useRef(overlay);
  const eventsRef = useRef(events);
  eventsRef.current = events;
  useEffect(() => {
    const element = canvas.current;
    if (element === null) return;
    let renderer: Renderer;
    try {
      renderer = new Renderer(element);
    } catch (error) {
      setProblem(error instanceof NoWebGL ? "This browser cannot draw with WebGL2." : String(error));
      return;
    }
    const around = surroundings(element);
    const controller = new Controller(renderer, around);
    const state: World = { controller, renderer, catalogue: undefined, domains: new Map(), rotations: undefined, pickable: [], chosen: undefined };
    setWorld(state);
    const abort = new AbortController();
    startCatalogue(release, state, abort.signal).catch((error: unknown) => {
      if (!abort.signal.aborted) setProblem(`The tiles of release ${release} could not be read: ${String(error)}`);
    });
    const unsubscribe = controller.subscribe(() => state.catalogue?.update(controller.eye));
    const unbind = bindView(element, state, eventsRef);
    let frame = requestAnimationFrame(function loop() {
      if (controller.tick()) {
        placeOverlay(state, overlayRef.current, around.size());
        setVersion((n) => n + 1);
      }
      frame = requestAnimationFrame(loop);
    });
    return () => {
      abort.abort();
      cancelAnimationFrame(frame);
      unbind();
      unsubscribe();
      renderer.dispose();
      setWorld(undefined);
      setTimeout(() => {
        if (!element.isConnected) renderer.loseContext();
      });
    };
  }, [release, canvas]);
  return { world, version, overlay, problem };
}
