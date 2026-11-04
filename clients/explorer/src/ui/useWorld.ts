// Starts the scene for a release on a canvas: the renderer, the view's controller, the
// catalogue of tiles and the mouse, and keeps the overlay (Earth, the ring's label) in place.

import { type RefObject, useEffect, useRef, useState } from "react";

import { Catalogue } from "../catalogue/catalogue.ts";
import { type Batch, domainsOf } from "../catalogue/tiles.ts";
import { project } from "../scene/camera.ts";
import { Controller, type Surroundings } from "../scene/controller.ts";
import { shortDistance } from "../scene/format.ts";
import { ringAnchor } from "../scene/frames.ts";
import { bindMouse } from "../scene/input.ts";
import { NoWebGL, Renderer } from "../scene/renderer.ts";
import { ORIGIN, type Vec3 } from "../scene/vec.ts";

export interface World {
  controller: Controller;
  catalogue: Catalogue | undefined;
}

/** Overlay elements kept over the points they name. */
export interface Overlay {
  earth: RefObject<HTMLDivElement | null>;
  ring: RefObject<HTMLDivElement | null>;
  ringLabel: RefObject<HTMLSpanElement | null>;
}

function place(element: HTMLElement | null, at: { x: number; y: number } | undefined): void {
  if (element === null) return;
  element.style.display = at === undefined ? "none" : "";
  if (at !== undefined) element.style.transform = `translate(${at.x}px, ${at.y}px)`;
}

function surroundings(element: HTMLElement): Surroundings {
  return {
    size: () => ({ width: element.clientWidth, height: element.clientHeight, ratio: devicePixelRatio }),
    now: () => performance.now(),
    reducedMotion: () => matchMedia("(prefers-reduced-motion: reduce)").matches,
  };
}

/** Reads the release's domains and starts fetching its tiles into `world`. */
function startCatalogue(release: number, renderer: Renderer, world: World, signal: AbortSignal): Promise<void> {
  const { controller } = world;
  return domainsOf(release, { signal }).then((domains) => {
    const first = domains[0];
    if (first === undefined) return;
    controller.setRotations(first.manifest.rotations);
    const events = {
      onBatch: (batch: Batch) => {
        renderer.addPoints({ centre: batch.tile.centre, offsets: batch.offsets, kinds: batch.kinds });
        controller.invalidate();
      },
      onChange: () => controller.invalidate(),
    };
    const catalogue = new Catalogue(release, domains, events, { signal });
    world.catalogue = catalogue;
    catalogue.update(controller.eye);
  });
}

/** Binds the mouse and the canvas's size to the controller, until the returned function. */
function bindView(element: HTMLCanvasElement, controller: Controller): () => void {
  const unbind = bindMouse(element, {
    turn: (dx, dy) => controller.turn(dx, dy),
    pan: (dx, dy) => controller.pan(dx, dy),
    zoom: (steps) => controller.zoom(steps),
    click: () => undefined,
    doubleClick: () => undefined,
    hover: () => undefined,
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
  };
}

/** Keeps Earth's marker and the ring's label over their places in the view. */
function placeOverlay(controller: Controller, overlay: Overlay, size: Surroundings["size"]): void {
  const { width, height } = size();
  const at = (p: Vec3) => project(controller.view, controller.toFrame, p, width, height);
  place(overlay.earth.current, at(ORIGIN));
  place(overlay.ring.current, at(ringAnchor(controller.toFrame, controller.ringRadius)));
  const label = overlay.ringLabel.current;
  const text = shortDistance(controller.ringRadius);
  if (label !== null && label.textContent !== text) label.textContent = text;
}

/** The scene of `release` on `canvas`: its world, a count that moves on each frame drawn (for
 * the panels), the overlay's elements, and the problem that stopped it, if any. */
export function useWorld(
  release: number,
  canvas: RefObject<HTMLCanvasElement | null>,
): { world: World | undefined; version: number; overlay: Overlay; problem: string | undefined } {
  const [version, setVersion] = useState(0);
  const [problem, setProblem] = useState<string>();
  const [world, setWorld] = useState<World>();
  const overlay: Overlay = { earth: useRef(null), ring: useRef(null), ringLabel: useRef(null) };
  const overlayRef = useRef(overlay);
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
    const state: World = { controller, catalogue: undefined };
    setWorld(state);
    const abort = new AbortController();
    startCatalogue(release, renderer, state, abort.signal).catch((error: unknown) => {
      if (!abort.signal.aborted) setProblem(`The tiles of release ${release} could not be read: ${String(error)}`);
    });
    const unsubscribe = controller.subscribe(() => state.catalogue?.update(controller.eye));
    const unbind = bindView(element, controller);
    let frame = requestAnimationFrame(function loop() {
      if (controller.tick()) {
        placeOverlay(controller, overlayRef.current, around.size);
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
