import { describe, expect, it } from "vitest";

import { fakeWorld } from "../test/world.ts";
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
  return { earth: { current: div() }, ring: { current: div() }, ringLabel: { current: document.createElement("span") }, chosen: { current: div() }, names: { current: container } };
}

const numbers = (element: Element | null | undefined) =>
  ((element as HTMLElement | null | undefined)?.style.transform.match(/-?[\d.]+/g) ?? []).map(Number);

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
