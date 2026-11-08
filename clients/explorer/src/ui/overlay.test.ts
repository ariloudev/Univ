import { describe, expect, it } from "vitest";

import { fakeWorld } from "../test/world.ts";
import { project } from "../scene/camera.ts";
import { FADE } from "../scene/drawing.ts";
import { ringSpacing } from "../scene/rings.ts";
import type { Vec3 } from "../scene/vec.ts";
import { FLOW_PIXELS } from "../scene/kinds.ts";
import { FLOW_REACH } from "../scene/outline.ts";
import { type Overlay, labelSize, nameAt, placeOverlay } from "./overlay.ts";
import { SYMBOL_REACH } from "./useStructures.ts";

const div = () => document.createElement("div");

function overlay(names: string[]): Overlay {
  const container = div();
  for (const id of names) {
    const element = div();
    element.dataset["structure"] = id;
    container.append(element);
  }
  return {
    earth: { current: div() },
    ring: { current: div() },
    ringLabel: { current: document.createElement("span") },
    ringLabels: { current: div() },
    chosen: { current: div() },
    names: { current: container },
  };
}

const numbers = (element: Element | null | undefined) =>
  ((element as HTMLElement | null | undefined)?.style.transform.match(/-?[\d.]+/g) ?? []).map(Number);

/** The elements of `container` shown. */
const shownIn = (container: HTMLElement | null) => [...(container?.children ?? [])].filter((e) => (e as HTMLElement).style.display !== "none");

/** Whether the labels `elements`, each placed by its top left corner, overlap none of the others. */
function apart(elements: readonly Element[]): boolean {
  const boxes = elements.map((e) => {
    const [x = 0, y = 0] = numbers(e);
    return { x, y, width: labelSize(e.textContent ?? "", "ring").width };
  });
  return boxes.every((a, i) => boxes.slice(i + 1).every((b) => Math.abs(a.x - b.x) >= Math.min(a.width, b.width) - 1e-6 || Math.abs(a.y - b.y) >= 16 - 1e-6));
}

/** Whether the ring label `element`, placed by its top left corner, clears the name's box `name`. */
function clearOf(element: Element, name: { x: number; y: number; width: number; height: number }): boolean {
  const [x = 0, y = 0] = numbers(element);
  const width = labelSize(element.textContent ?? "", "ring").width;
  return x + width <= name.x - name.width / 2 || x >= name.x + name.width / 2 || y + 16 <= name.y - name.height / 2 || y >= name.y + name.height / 2;
}

describe("the overlay", () => {
  it("keeps Earth at the centre while it is the focus, and marks the chosen object where it is", () => {
    const marks = overlay([]);
    placeOverlay(fakeWorld({ chosen: [0, 0, 0] }), marks, { width: 800, height: 600 });
    const [x, y] = numbers(marks.earth.current);
    expect(x).toBeCloseTo(400);
    expect(y).toBeCloseTo(300);
    expect(marks.chosen.current?.style.display).toBe("");
    expect(marks.ringLabel.current?.textContent).toBe("1 Gpc");
    placeOverlay(fakeWorld(), marks, { width: 800, height: 600 });
    expect(marks.chosen.current?.style.display).toBe("none");
  });

  it("labels each distance ring where it crosses the spoke of zero longitude, in place of the equator ring's label, none overlapping", () => {
    const marks = overlay([]);
    const world = fakeWorld();
    const ringsOn = (on: boolean) => {
      world.controller.setRings(on);
      world.controller.tick();
      placeOverlay(world, marks, { width: 800, height: 600 });
    };
    // Turned so that the equator ring's label at zero longitude is in sight.
    world.controller.turn(-300, 0);
    ringsOn(false);
    expect(marks.ring.current?.style.display).toBe("");
    ringsOn(true);
    expect(marks.ring.current?.style.display).toBe("none");
    const shown = shownIn(marks.ringLabels.current);
    expect(shown.length).toBeGreaterThan(2);
    expect(shown[0]?.textContent).toMatch(/^[\d.,]+ (pc|kpc|Mpc|Gpc)$/);
    expect(apart(shown)).toBe(true);
    // Off again: the ring's label is back and the rings' labels are gone.
    ringsOn(false);
    expect(marks.ring.current?.style.display).toBe("");
    expect(shownIn(marks.ringLabels.current)).toHaveLength(0);
  });

  it("hides the equator ring's label as soon as the rings are on, though none is yet drawn, and keeps the rings' labels clear of the structures' names", () => {
    const marks = overlay(["kbc"]);
    const world = fakeWorld({ names: [{ id: "kbc", centre: [0, 0, 0], text: "KBC void" }] });
    world.controller.turn(-300, 0);
    world.controller.setRings(true);
    placeOverlay(world, marks, { width: 800, height: 600 });
    expect(world.controller.rings).toBeUndefined();
    expect(marks.ring.current?.style.display).toBe("none");
    world.controller.tick();
    placeOverlay(world, marks, { width: 800, height: 600 });
    const [name] = world.namesShown;
    if (name === undefined) throw new Error("no name shown");
    const shown = shownIn(marks.ringLabels.current);
    expect(shown.length).toBeGreaterThan(0);
    expect(shown.every((element) => clearOf(element, name))).toBe(true);
  });

  it("leaves out the labels of the rings that fade where they crowd toward the horizon, and keeps the near ones", () => {
    const marks = overlay([]);
    const world = fakeWorld();
    // Low over the plane, looking out along the spoke of zero longitude.
    world.controller.turn(-450, 0);
    world.controller.setRings(true);
    world.controller.tick();
    placeOverlay(world, marks, { width: 800, height: 600 });
    const { controller } = world;
    const at = (p: Vec3) => project(controller.view, controller.toFrame, p, 800, 600);
    const step = controller.rings?.step ?? NaN;
    const faded = new Set(controller.ringLabels.filter((l) => at(l.at) !== undefined && ringSpacing(at, l.at, step) < FADE.full).map((l) => l.text));
    expect(faded.size).toBeGreaterThan(0);
    const shown = shownIn(marks.ringLabels.current).map((e) => e.textContent ?? "");
    expect(shown.length).toBeGreaterThan(2);
    expect(shown.filter((text) => faded.has(text))).toEqual([]);
  });

  it("draws each structure's name at its centre", () => {
    const marks = overlay(["kbc"]);
    placeOverlay(fakeWorld({ names: [{ id: "kbc", centre: [0, 0, 0], text: "KBC void" }] }), marks, { width: 800, height: 600 });
    const [x, y] = numbers(marks.names.current?.firstElementChild);
    expect(x).toBeCloseTo(400);
    expect(y).toBeCloseTo(300);
  });

  it("draws an attractor's or a repeller's name below the reach of its symbol's arrows, where the mouse chooses it", () => {
    const marks = overlay(["ga"]);
    const world = fakeWorld({ names: [{ id: "ga", centre: [0, 0, 0], text: "Great Attractor", below: SYMBOL_REACH }] });
    placeOverlay(world, marks, { width: 800, height: 600, ratio: 2 });
    const [x, y = 0] = numbers(marks.names.current?.firstElementChild);
    expect(x).toBeCloseTo(400);
    // Its box's top clears the arrows' reach, in CSS pixels on a screen of two device pixels to each.
    expect(y - labelSize("Great Attractor").height / 2 - 300).toBeGreaterThan((FLOW_REACH * FLOW_PIXELS) / 2);
    expect(nameAt(world, 400, y)?.id).toBe("ga");
    expect(nameAt(world, 400, 300)).toBeUndefined();
  });

  it("lets the mouse choose a structure by the name it shows", () => {
    const world = fakeWorld({ names: [{ id: "kbc", centre: [0, 0, 0], text: "KBC void" }] });
    placeOverlay(world, overlay(["kbc"]), { width: 800, height: 600 });
    expect(nameAt(world, 405, 302)?.id).toBe("kbc");
    expect(nameAt(world, 600, 302)).toBeUndefined();
  });

  it("hides a structure's name that would overlap a nearer one", () => {
    const marks = overlay(["near", "far"]);
    const names = [
      { id: "near", centre: [0, 0, 0] as Vec3, text: "A long structure name" },
      { id: "far", centre: [-1, 0, 0] as Vec3, text: "Another long name" },
    ];
    placeOverlay(fakeWorld({ names }), marks, { width: 800, height: 600 });
    const shown = [...(marks.names.current?.children ?? [])].map((e) => (e as HTMLElement).style.display);
    expect(shown.sort()).toEqual(["", "none"]);
    expect(labelSize("KBC void").width).toBeGreaterThan(50);
  });
});
