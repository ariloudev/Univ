import { describe, expect, it } from "vitest";

import type { Overlay } from "./overlay.ts";
import { FIRST_POINTS } from "../firstPoints.ts";
import { fakeWorld } from "../test/world.ts";
import { DOUBLE_CLICK_MS, drawn, firstOfDouble } from "./useWorld.ts";

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
    expect(clicked(100, 100)).toBe("the point aimed at");
    clock += 120;
    expect(clicked(101, 100)).toBe("the point aimed at");
    clock += 80;
    expect(clicked(101, 101)).toBe("the point aimed at");
    expect(picks).toEqual(["the point aimed at"]);
    clock += DOUBLE_CLICK_MS + 1;
    expect(clicked(101, 101)).toBe("a point the view moved under the pointer");
  });

  it("picks again for a click elsewhere", () => {
    const clicked = firstOfDouble((x) => `at ${x}`, () => 0);
    expect(clicked(100, 100)).toBe("at 100");
    expect(clicked(300, 100)).toBe("at 300");
  });
});
