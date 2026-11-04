// Starts the scene for a release on a canvas: the renderer, the view's controller, the
// catalogue of tiles and the mouse, and keeps the overlay (Earth, the ring's label, the chosen
// object's mark, the structures' names) in place.

import { type RefObject, useEffect, useRef, useState } from "react";

import type { Rotations } from "../api/schemas.ts";
import { Catalogue } from "../catalogue/catalogue.ts";
import { type Batch, domainsOf } from "../catalogue/tiles.ts";
import { Controller, type Surroundings } from "../scene/controller.ts";
import { viewingDistance } from "../scene/controls.ts";
import { bindMouse } from "../scene/input.ts";
import type { Pickable } from "../scene/pick.ts";
import { NoWebGL, Renderer } from "../scene/renderer.ts";
import type { Vec3 } from "../scene/vec.ts";
import { FIRST_POINTS } from "../firstPoints.ts";
import type { LabelBox } from "../scene/labels.ts";
import { type NameLabel, type Overlay, nameAt, placeOverlay } from "./overlay.ts";
import { type ScenePoint, pointUnder } from "./scenePoint.ts";
import { type Store, createStore } from "./store.ts";

/** What is under the pointer, where the pointer is: a structure's name, which a click
 * chooses, or else a point. */
export type Hover = ({ x: number; y: number } & ({ name: NameLabel } | { point: ScenePoint })) | undefined;

/** What is under the pointer at `x`, `y`: a name before a point, as a click chooses. */
function under(world: World, element: HTMLCanvasElement, x: number, y: number): Hover {
  const name = nameAt(world, x, y);
  if (name !== undefined) return { x, y, name };
  const point = pointAt(world, element, x, y);
  return point === undefined ? undefined : { x, y, point };
}

export interface World {
  controller: Controller;
  renderer: Renderer;
  catalogue: Catalogue | undefined;
  /** Every batch drawn, to pick from. */
  pickable: (Pickable & { batch: Batch })[];
  /** Where the chosen object is marked. */
  chosen: Vec3 | undefined;
  /** The structures' names, to place over the view. */
  names: NameLabel[];
  /** The boxes of the names shown on the last frame drawn, for the mouse to choose by. */
  namesShown: LabelBox[];
  /** Counts the frames drawn, for what shows the view's state. */
  frame: Store<number>;
  hover: Store<Hover>;
}

/** What the scene tells the page of the mouse: a point clicked, or double-clicked and flown to;
 * a structure whose name was clicked. */
export interface OnChoose {
  point: (point: ScenePoint) => void;
  structure: (id: string) => void;
}

function surroundings(element: HTMLElement): Surroundings {
  return {
    size: () => ({ width: element.clientWidth, height: element.clientHeight, ratio: devicePixelRatio }),
    now: () => performance.now(),
    reducedMotion: () => matchMedia("(prefers-reduced-motion: reduce)").matches,
  };
}

/** A world for `controller` drawing with `renderer`, holding nothing yet. */
export function emptyWorld(controller: Controller, renderer: Renderer): World {
  return {
    controller,
    renderer,
    catalogue: undefined,
    pickable: [],
    chosen: undefined,
    names: [],
    namesShown: [],
    frame: createStore(0),
    hover: createStore<Hover>(undefined),
  };
}

/** Reads the release's domains and starts fetching its tiles into `world`; its rotations. */
async function startCatalogue(release: number, world: World, signal: AbortSignal): Promise<Rotations | undefined> {
  const { controller, renderer } = world;
  const domains = await domainsOf(release, { signal });
  const rotations = domains[0]?.manifest.rotations;
  if (rotations === undefined) return undefined;
  controller.setRotations(rotations);
  const events = {
    onBatch: (batch: Batch) => {
      renderer.addPoints({ centre: batch.tile.centre, offsets: batch.offsets, kinds: batch.kinds });
      // The bounding sphere of the tile's cube: half its diagonal.
      world.pickable.push({ centre: batch.tile.centre, radius: batch.tile.edge * (Math.sqrt(3) / 2), offsets: batch.offsets, batch });
      controller.invalidate();
    },
    onChange: () => controller.invalidate(),
  };
  world.catalogue = new Catalogue(release, domains, events, { signal });
  world.catalogue.update(controller.eye);
  return rotations;
}

/** The point of `world` under the pointer at `x`, `y` on `element`, if any. */
const pointAt = (world: World, element: HTMLElement, x: number, y: number): ScenePoint | undefined =>
  pointUnder(world, { width: element.clientWidth, height: element.clientHeight, x, y });

/** Binds the mouse and the canvas's size to the world, until the returned function. */
function bindView(element: HTMLCanvasElement, world: World, onChoose: RefObject<OnChoose>): () => void {
  const { controller, hover } = world;
  let hovering: number | undefined;
  const chooser = (fly: boolean) => (x: number, y: number) => {
    const found = under(world, element, x, y);
    if (found === undefined) return;
    if ("name" in found) {
      onChoose.current.structure(found.name.id);
      return;
    }
    if (fly) controller.flyTo(found.point.place, viewingDistance(found.point.distanceMpc));
    onChoose.current.point(found.point);
  };
  const unbind = bindMouse(element, {
    turn: (dx, dy) => controller.turn(dx, dy),
    pan: (dx, dy) => controller.pan(dx, dy),
    zoom: (steps) => controller.zoom(steps),
    click: chooser(false),
    doubleClick: chooser(true),
    hover: (x, y) => {
      if (hovering !== undefined) cancelAnimationFrame(hovering);
      hovering = requestAnimationFrame(() => hover.set(under(world, element, x, y)));
    },
    leave: () => {
      if (hovering !== undefined) cancelAnimationFrame(hovering);
      if (hover.get() !== undefined) hover.set(undefined);
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

/** What follows a frame drawn: the overlay placed, the frame counted, and the first points
 * marked if these are they; whether the first points are now marked. */
export function drawn(world: World, overlay: Overlay, size: { width: number; height: number }, marked: boolean): boolean {
  placeOverlay(world, overlay, size);
  world.frame.set(world.frame.get() + 1);
  if (marked || world.pickable.length === 0) return marked;
  performance.mark(FIRST_POINTS);
  return true;
}

/** Draws `world` on every animation frame that needs it, placing its overlay, until stopped. */
function animate(world: World, overlay: RefObject<Overlay>, size: Surroundings["size"]): () => void {
  let marked = false;
  let frame = requestAnimationFrame(function loop() {
    if (world.controller.tick()) marked = drawn(world, overlay.current, size(), marked);
    frame = requestAnimationFrame(loop);
  });
  return () => cancelAnimationFrame(frame);
}

/** The scene of `release` on `canvas`: its world, the rotations of its frames once read, the
 * overlay's elements, and the problem that stopped it, if any. */
export function useWorld(release: number, canvas: RefObject<HTMLCanvasElement | null>, onChoose: OnChoose) {
  const [problem, setProblem] = useState<string>();
  const [world, setWorld] = useState<World>();
  const [rotations, setRotations] = useState<Rotations>();
  const overlay: Overlay = { earth: useRef(null), ring: useRef(null), ringLabel: useRef(null), chosen: useRef(null), names: useRef(null) };
  const overlayRef = useRef(overlay);
  const chooseRef = useRef(onChoose);
  chooseRef.current = onChoose;
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
    const state = emptyWorld(new Controller(renderer, around), renderer);
    setWorld(state);
    const abort = new AbortController();
    startCatalogue(release, state, abort.signal)
      .then(setRotations)
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setProblem(`The tiles of release ${release} could not be read: ${String(error)}`);
      });
    const unsubscribe = state.controller.subscribe(() => state.catalogue?.update(state.controller.eye));
    const unbind = bindView(element, state, chooseRef);
    const stop = animate(state, overlayRef, around.size);
    return () => {
      abort.abort();
      stop();
      unbind();
      unsubscribe();
      renderer.dispose();
      setWorld(undefined);
      setRotations(undefined);
      setTimeout(() => {
        if (!element.isConnected) renderer.loseContext();
      });
    };
  }, [release, canvas]);
  return { world, rotations, overlay, problem };
}
