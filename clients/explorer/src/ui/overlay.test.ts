import { describe, expect, it } from "vitest";

import { Controller } from "../scene/controller.ts";
import type { Vec3 } from "../scene/vec.ts";
import { type Overlay, placeOverlay } from "./overlay.ts";
import type { World } from "./useWorld.ts";

function element(width = 0): HTMLDivElement {
  const made = document.createElement("div");
  Object.defineProperty(made, "offsetWidth", { value: width });
  Object.defineProperty(made, "offsetHeight", { value: 14 });
  return made;
}

function scene(chosen: Vec3 | undefined): World {
  const controller = new Controller(
    { draw: () => undefined, setLines: () => undefined },
    { size: () => ({ width: 800, height: 600, ratio: 1 }), now: () => 0, reducedMotion: () => true },
  );
  return { controller, renderer: {} as World["renderer"], catalogue: undefined, domains: new Map(), rotations: undefined, pickable: [], chosen };
}

function overlay(labels: [string, HTMLElement, Vec3][]): Overlay {
  return {
    earth: { current: element() },
    ring: { current: element() },
    ringLabel: { current: document.createElement("span") },
    chosen: { current: element() },
    labels: { current: new Map(labels.map(([id, e, centre]) => [id, { element: e, centre }])) },
  };
}

describe("the overlay", () => {
  it("keeps Earth at the centre while it is the focus, and marks the chosen object where it is", () => {
    const marks = overlay([]);
    placeOverlay(scene([0, 0, 0]), marks, { width: 800, height: 600 });
    const [x, y] = (marks.earth.current?.style.transform.match(/-?[\d.]+/g) ?? []).map(Number);
    expect(x).toBeCloseTo(400);
    expect(y).toBeCloseTo(300);
    expect(marks.chosen.current?.style.display).toBe("");
    expect(marks.ringLabel.current?.textContent).toBe("1 Gpc");
    placeOverlay(scene(undefined), marks, { width: 800, height: 600 });
    expect(marks.chosen.current?.style.display).toBe("none");
  });

  it("hides a structure's name that would overlap a nearer one", () => {
    const near = element(120);
    const far = element(120);
    const marks = overlay([
      ["near", near, [0, 0, 0]],
      ["far", far, [-1, 0, 0]],
    ]);
    placeOverlay(scene(undefined), marks, { width: 800, height: 600 });
    expect([near.style.display, far.style.display].sort()).toEqual(["", "none"]);
  });
});
