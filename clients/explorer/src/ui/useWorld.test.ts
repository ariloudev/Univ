import { describe, expect, it } from "vitest";

import { fakeWorld } from "../test/world.ts";
import type { Overlay } from "./overlay.ts";
import { FIRST_POINTS } from "../firstPoints.ts";
import { drawn } from "./useWorld.ts";

const overlay: Overlay = {
  earth: { current: null },
  ring: { current: null },
  ringLabel: { current: null },
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
