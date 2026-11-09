import { describe, expect, it } from "vitest";

import type { Overlay } from "./overlay.ts";
import { FIRST_POINTS } from "../firstPoints.ts";
import { fakeWorld } from "../test/world.ts";
import { DOUBLE_CLICK_MS, chooses, drawn, firstOfDouble } from "./useWorld.ts";

const overlay: Overlay = {
  earth: { current: null },
  ring: { current: null },
  ringLabel: { current: null },
  ringLabels: { current: null },
  chosen: { current: null },
  names: { current: null },
};
const SIZE = { width: 800, height: 600 };

describe("a frame drawn", () => {
  it("marks the first points once, on the first frame that holds any, and counts every frame", () => {
    performance.clearMarks(FIRST_POINTS);
    const world = fakeWorld();
    expect(drawn(world, overlay, SIZE, false)).toBe(false);
    expect(performance.getEntriesByName(FIRST_POINTS)).toHaveLength(0);
    world.pickable.push({ centre: [0, 0, 0], radius: 1, offsets: new Float32Array(3), batch: undefined as never });
    expect(drawn(world, overlay, SIZE, false)).toBe(true);
    expect(drawn(world, overlay, SIZE, true)).toBe(true);
    expect(performance.getEntriesByName(FIRST_POINTS)).toHaveLength(1);
    expect(world.frame.get()).toBe(3);
  });
});

describe("a double-click", () => {
  it("chooses what its first click found, though that click has moved the view", () => {
    let clock = 0;
    const under = ["the point aimed at", "a point the view moved under the pointer"];
    const picks: string[] = [];
    const clicked = firstOfDouble(() => {
      const found = under.shift();
      if (found !== undefined) picks.push(found);
      return found;
    }, () => clock);
    expect(clicked(100, 100)).toEqual({ found: "the point aimed at", again: false });
    clock += 120;
    expect(clicked(101, 100)).toEqual({ found: "the point aimed at", again: true });
    clock += 80;
    expect(clicked(101, 101)).toEqual({ found: "the point aimed at", again: true });
    expect(picks).toEqual(["the point aimed at"]);
    clock += DOUBLE_CLICK_MS + 1;
    expect(clicked(101, 101)).toEqual({ found: "a point the view moved under the pointer", again: false });
  });

  it("picks again for a click elsewhere", () => {
    const clicked = firstOfDouble((x) => `at ${x}`, () => 0);
    expect(clicked(100, 100)).toEqual({ found: "at 100", again: false });
    expect(clicked(300, 100)).toEqual({ found: "at 300", again: false });
  });

  it("finds nothing where nothing is under the pointer, and a click there starts no double-click", () => {
    let clock = 0;
    const under: (string | undefined)[] = [undefined, "a point"];
    const clicked = firstOfDouble(() => under.shift(), () => clock);
    expect(clicked(100, 100)).toBeUndefined();
    clock += 100;
    expect(clicked(100, 100)).toEqual({ found: "a point", again: false });
  });
});

describe("a click on what is chosen already (read-an-object, see-the-structures)", () => {
  const letGoOf = (chosen: string) => {
    const letGo: string[] = [];
    return { letGo, of: (found: string) => found === chosen && letGo.push(found) > 0 };
  };

  it("lets it go, and chooses nothing", () => {
    const { letGo, of } = letGoOf("the chosen point");
    expect(chooses({ found: "the chosen point", again: false }, false, of)).toBe(false);
    expect(letGo).toEqual(["the chosen point"]);
  });

  it("chooses anything else", () => {
    const { letGo, of } = letGoOf("the chosen point");
    expect(chooses({ found: "another point", again: false }, false, of)).toBe(true);
    expect(letGo).toEqual([]);
  });

  it("leaves the second click of a double-click to the double-click, letting nothing go", () => {
    const { letGo, of } = letGoOf("a point");
    expect(chooses({ found: "a point", again: true }, false, of)).toBe(false);
    expect(letGo).toEqual([]);
  });

  it("is chosen by a double-click, which lets nothing go", () => {
    const { letGo, of } = letGoOf("the chosen point");
    expect(chooses({ found: "the chosen point", again: true }, true, of)).toBe(true);
    expect(letGo).toEqual([]);
  });
});
