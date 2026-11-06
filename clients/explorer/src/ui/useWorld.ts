// Starts the scene for a release on a canvas: the renderer, the view's controller, the
// catalogue of tiles and the mouse, and keeps the overlay (Earth, the ring's label, the chosen
// object's mark, the structures' names) in place.

import { type RefObject, useEffect, useRef, useState } from "react";

import type { Rotations } from "../api/schemas.ts";
import { Catalogue } from "../catalogue/catalogue.ts";
import { EVERYTHING } from "../catalogue/lod.ts";
import { type Batch, type Domain, domainsOf } from "../catalogue/tiles.ts";
import { Controller, type Surroundings } from "../scene/controller.ts";
import { viewingDistance } from "../scene/controls.ts";
import { bindMouse } from "../scene/input.ts";
import { type Masks, NO_MASKS, masksOf, shownIn } from "../scene/masks.ts";
import { PICK_SLOP, type Pickable } from "../scene/pick.ts";
import { NoWebGL, Renderer } from "../scene/renderer.ts";
import { type Vec3, norm, sub } from "../scene/vec.ts";
import { FIRST_POINTS } from "../firstPoints.ts";
import type { LabelBox } from "../scene/labels.ts";
import { type NameLabel, type Overlay, nameAt, placeOverlay } from "./overlay.ts";
import type { Hidden } from "./hidden.ts";
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
  /** What is turned off, as the renderer, the pick and the key read it. */
  masks: Masks;
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
    masks: NO_MASKS,
    chosen: undefined,
    names: [],
    namesShown: [],
    frame: createStore(0),
    hover: createStore<Hover>(undefined),
  };
}

/** Which of `batch`'s points `masks` leave drawn. */
const shownOf = (masks: Masks, batch: Batch) => shownIn(masks, batch.tile.domain, batch.sources, batch.kinds);

/** Turns off the points of the groups `hidden` names in `world`, of `domains`: drawn, picked
 * and counted alike. */
export function hide(world: World, hidden: Pick<Hidden, "domain" | "source" | "kind">, domains: readonly Domain[]): void {
  world.masks = masksOf(hidden, domains);
  world.renderer.setHidden(world.masks);
  for (const p of world.pickable) p.shown = shownOf(world.masks, p.batch);
  world.catalogue?.setMasks(world.masks);
  world.controller.invalidate();
}

/** What a release's catalogue starts from: its domains, and the rotations of its frames. */
interface Started {
  domains: Domain[];
  rotations: Rotations;
}

/** Reads the release's domains and starts fetching its tiles into `world`; its domains and
 * rotations. */
async function startCatalogue(release: number, world: World, signal: AbortSignal): Promise<Started | undefined> {
  const { controller, renderer } = world;
  const domains = await domainsOf(release, { signal });
  const rotations = domains[0]?.manifest.rotations;
  if (rotations === undefined) return undefined;
  controller.setRotations(rotations);
  const events = {
    onBatch: (batch: Batch) => {
      const { id, centre } = batch.tile;
      const { extent } = batch;
      const { offsets, kinds, sources } = batch;
      const slice = batch.tile.slice ?? [0, 1];
      renderer.addPoints({ id, domain: batch.tile.domain, centre, extent, offsets, kinds, sources, colours: batch.stars?.bytes, slice });
      // Picking tests a sphere about the tile's centre: the one holding the points' own.
      const radius = norm(sub(extent.centre, centre)) + extent.radius;
      world.pickable.push({ centre, radius, offsets, batch, shown: shownOf(world.masks, batch) });
      controller.invalidate();
    },
    onDrop: (batch: Batch) => {
      renderer.removePoints(batch.tile.id);
      const at = world.pickable.findIndex((p) => p.batch === batch);
      if (at >= 0) world.pickable.splice(at, 1);
      controller.invalidate();
    },
    onChange: () => controller.invalidate(),
  };
  world.catalogue = new Catalogue(release, domains, events, { signal });
  world.catalogue.update(controller.sight);
  return { domains, rotations };
}

/** The point of `world` under the pointer at `x`, `y` on `element`, if any. */
const pointAt = (world: World, element: HTMLElement, x: number, y: number): ScenePoint | undefined =>
  pointUnder(world, { width: element.clientWidth, height: element.clientHeight, x, y });

/** The milliseconds within which a second click, or a double-click, near a click is part of it. */
export const DOUBLE_CLICK_MS = 500;

/** `pick` at the pointer, but for a click soon after another near it, the thing the other found:
 * a click centres the view on what it found, so a pick at the same pixel in a double-click would
 * find another thing, or none (navigate-with-the-mouse). */
export function firstOfDouble<T>(pick: (x: number, y: number) => T | undefined, now: () => number) {
  let last: { x: number; y: number; at: number; found: T } | undefined;
  return (x: number, y: number): T | undefined => {
    const at = now();
    if (last !== undefined && at - last.at <= DOUBLE_CLICK_MS && Math.hypot(x - last.x, y - last.y) <= PICK_SLOP) {
      last.at = at;
      return last.found;
    }
    const found = pick(x, y);
    last = found === undefined ? undefined : { x, y, at, found };
    return found;
  };
}

/** Binds the mouse and the canvas's size to the world, until the returned function. */
function bindView(element: HTMLCanvasElement, world: World, onChoose: RefObject<OnChoose>): () => void {
  const { controller, hover } = world;
  let hovering: number | undefined;
  /** Nothing under the pointer, nor any pick a pending frame would make. */
  const clearHover = () => {
    if (hovering !== undefined) cancelAnimationFrame(hovering);
    if (hover.get() !== undefined) hover.set(undefined);
  };
  const clicked = firstOfDouble((x, y) => under(world, element, x, y), () => performance.now());
  // A click centres the view on the point; a double-click flies in close to it.
  const chooser = (close: boolean) => (x: number, y: number) => {
    const found = clicked(x, y);
    if (found === undefined) return;
    if ("name" in found) {
      onChoose.current.structure(found.name.id);
      return;
    }
    if (close) controller.approach(found.point.place);
    else controller.centreOn(found.point.place);
    // The view moves: what was under the pointer is no longer.
    clearHover();
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
    leave: clearHover,
  });
  const resize = new ResizeObserver(() => {
    element.width = Math.round(element.clientWidth * devicePixelRatio);
    element.height = Math.round(element.clientHeight * devicePixelRatio);
    controller.invalidate();
    // A new size is a new frustum: what the view needs is worked out again.
    world.catalogue?.update(controller.sight);
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
    if (world.renderer.fading()) world.controller.invalidate();
    if (world.controller.tick()) marked = drawn(world, overlay.current, size(), marked);
    frame = requestAnimationFrame(loop);
  });
  return () => cancelAnimationFrame(frame);
}

/** The scene of `release` on `canvas`: its world, its domains and the rotations of its frames
 * once read, the overlay's elements, and the problem that stopped it, if any. */
export function useWorld(release: number, canvas: RefObject<HTMLCanvasElement | null>, onChoose: OnChoose) {
  const [problem, setProblem] = useState<string>();
  const [world, setWorld] = useState<World>();
  const [rotations, setRotations] = useState<Rotations>();
  const [domains, setDomains] = useState<readonly Domain[]>([]);
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
    renderer.setShare((id) => state.catalogue?.share(id) ?? 1);
    renderer.setDetail((domain) => state.catalogue?.detail(domain) ?? EVERYTHING);
    setWorld(state);
    const abort = new AbortController();
    startCatalogue(release, state, abort.signal)
      .then((started) => {
        setDomains(started?.domains ?? []);
        setRotations(started?.rotations);
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setProblem(`The tiles of release ${release} could not be read: ${String(error)}`);
      });
    const unsubscribe = state.controller.subscribe(() => state.catalogue?.update(state.controller.sight));
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
      setDomains([]);
      setTimeout(() => {
        if (!element.isConnected) renderer.loseContext();
      });
    };
  }, [release, canvas]);
  return { world, domains, rotations, overlay, problem };
}
