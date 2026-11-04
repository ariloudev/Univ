import { describe, expect, it } from "vitest";

import type { View } from "./camera.ts";
import { inSight, sightOf } from "./frustum.ts";
import type { Mat3 } from "./vec.ts";

const IDENTITY: Mat3 = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
];
// The eye 10 Mpc out along +x of the frame, looking back at the origin.
const VIEW: View = { focus: [0, 0, 0], distance: 10, yaw: 0, pitch: 0 };

describe("the view's frustum", () => {
  const sight = sightOf(VIEW, IDENTITY, 800, 600);

  it("holds what lies ahead of the eye, and not what lies behind it", () => {
    expect(sight.inView([0, 0, 0], 0.1)).toBe(true);
    expect(sight.inView([20, 0, 0], 1)).toBe(false);
    // Behind, but large enough to reach in front of the eye.
    expect(sight.inView([11, 0, 0], 2)).toBe(true);
  });

  it("leaves out a sphere wholly beyond a side, and keeps one that crosses it", () => {
    // At the origin, 10 Mpc from the eye, the half-height of the view is 10 tan 25° ≈ 4.66 Mpc.
    expect(sight.inView([0, 0, 6], 1)).toBe(false);
    expect(sight.inView([0, 0, 6], 1.5)).toBe(true);
    // The view is wider than it is tall: 6 Mpc to the side is within its half-width of 6.2 Mpc.
    expect(sight.inView([0, 6, 0], 0.01)).toBe(true);
    expect(sight.inView([0, 8, 0], 1)).toBe(false);
  });

  it("keeps, of a list, only what reaches into the view", () => {
    const items = [
      { id: "ahead", centre: [0, 0, 0] as const, radius: 1 },
      { id: "behind", centre: [30, 0, 0] as const, radius: 1 },
    ].map((i) => ({ ...i, centre: [...i.centre] as [number, number, number] }));
    expect(inSight(items, sight).map((i) => i.id)).toEqual(["ahead"]);
  });

  it("gives the focal length in pixels", () => {
    expect(sight.focal).toBeCloseTo(300 / Math.tan((25 * Math.PI) / 180));
  });
});
