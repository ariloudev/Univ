// A view and a world for tests: a controller drawing nowhere on an 800 × 600 viewport, with
// reduced motion so that flights end at once.

import { Controller } from "../scene/controller.ts";
import type { Renderer } from "../scene/renderer.ts";
import { type World, emptyWorld } from "../ui/useWorld.ts";

export function fakeController(): Controller {
  return new Controller(
    { draw: () => undefined, setLines: () => undefined },
    { size: () => ({ width: 800, height: 600, ratio: 1 }), now: () => 0, reducedMotion: () => true },
  );
}

/** A world holding nothing, its renderer drawing nowhere, `overrides` set. */
export function fakeWorld(overrides: Partial<World> = {}): World {
  const renderer = { setLines: () => undefined, setMarkers: () => undefined, choose: () => undefined } as unknown as Renderer;
  return { ...emptyWorld(fakeController(), renderer), ...overrides };
}
