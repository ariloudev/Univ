import { describe, expect, it } from "vitest";

import { fakeWorld } from "../test/world.ts";
import type { Vec3 } from "../scene/vec.ts";
import { type Overlay, labelSize, placeOverlay } from "./overlay.ts";

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
