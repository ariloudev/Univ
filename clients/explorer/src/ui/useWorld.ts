// Starts the scene for a release on a canvas: the renderer, the view's controller, the
// catalogue of tiles and the mouse, and keeps the overlay (Earth, the ring's label) in place.

import { type RefObject, useEffect, useRef, useState } from "react";

import { Catalogue } from "../catalogue/catalogue.ts";
import { domainsOf } from "../catalogue/tiles.ts";
import { eyeOf, project } from "../scene/camera.ts";
import { Controller } from "../scene/controller.ts";
import type { Rotations } from "../scene/frames.ts";
import { bindMouse } from "../scene/input.ts";
import { shortDistance } from "../scene/format.ts";
import { coloursOf } from "../scene/kinds.ts";
import { NoWebGL, Renderer } from "../scene/renderer.ts";
import { ORIGIN, type Vec3, apply, transpose } from "../scene/vec.ts";

export interface World {
  controller: Controller;
  catalogue: Catalogue | undefined;
}

/** Overlay elements kept over the points they name. */
export interface Overlay {
  earth: RefObject<HTMLDivElement | null>;
  ring: RefObject<HTMLDivElement | null>;
}

/** How often the panels read the scene again, milliseconds. */
const PANEL_MS = 200;

function place(element: HTMLElement | null, at: { x: number; y: number } | undefined): void {
  if (element === null) return;
  element.style.display = at === undefined ? "none" : "";
  if (at !== undefined) element.style.transform = `translate(${at.x}px, ${at.y}px)`;
}

/** The scene of `release` on `canvas`: its world, a count that moves when the panels should read
 * it again, and the problem that stopped it, if any. */
export function useWorld(
  release: number,
  canvas: RefObject<HTMLCanvasElement | null>,
  overlay: Overlay,
): { world: World | undefined; tick: number; problem: string | undefined } {
  const [tick, setTick] = useState(0);
  const [problem, setProblem] = useState<string>();
  const world = useRef<World>(undefined);
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
    const size = () => ({ width: element.clientWidth, height: element.clientHeight, ratio: devicePixelRatio });
    const drawer = {
      draw: renderer.draw.bind(renderer),
      setLines: renderer.setLines.bind(renderer),
    };
    const controller = new Controller(drawer, {
      size,
      now: () => performance.now(),
      reducedMotion: () => matchMedia("(prefers-reduced-motion: reduce)").matches,
    });
    const state: World = { controller, catalogue: undefined };
    world.current = state;
    const abort = new AbortController();
    let panelTimer: number | undefined;
    const refreshPanels = () => {
      panelTimer ??= window.setTimeout(() => {
        panelTimer = undefined;
        setTick((n) => n + 1);
      }, PANEL_MS);
    };
    const eye = () => eyeOf(controller.view, controller.toFrame);
    domainsOf(release, { signal: abort.signal })
      .then((domains) => {
        const first = domains[0];
        if (first === undefined) return;
        controller.setRotations((first.manifest as unknown as { rotations: Rotations }).rotations);
        renderer.setColours(coloursOf(first.manifest.encoding.kinds));
        state.catalogue = new Catalogue(
          release,
          domains,
          {
            onBatch: (batch) => {
              renderer.addPoints(batch);
              controller.invalidate();
            },
            onChange: refreshPanels,
          },
          { signal: abort.signal },
        );
        state.catalogue.update(eye());
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setProblem(`The tiles of release ${release} could not be read: ${String(error)}`);
      });
    const unsubscribe = controller.subscribe(() => {
      state.catalogue?.update(eye());
      refreshPanels();
    });
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
    let frame = requestAnimationFrame(function loop() {
      controller.frame();
      const { width, height } = size();
      const at = (p: Vec3) => project(controller.view, controller.toFrame, p, width, height);
      place(overlay.earth.current, at(ORIGIN));
      place(overlay.ring.current, at(apply(transpose(controller.toFrame), [controller.ringRadius, 0, 0])));
      const label = overlay.ring.current?.lastElementChild;
      if (label instanceof HTMLElement) label.textContent = shortDistance(controller.ringRadius);
      frame = requestAnimationFrame(loop);
    });
    return () => {
      abort.abort();
      cancelAnimationFrame(frame);
      window.clearTimeout(panelTimer);
      resize.disconnect();
      renderer.clearPoints();
      unbind();
      unsubscribe();
      world.current = undefined;
    };
  }, [release, canvas, overlay]);
  return { world: world.current, tick, problem };
}
