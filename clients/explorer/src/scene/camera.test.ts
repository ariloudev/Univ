import { describe, expect, it } from "vitest";

import { MAX_PITCH, type View, eyeOf, project } from "./camera.ts";
import { turn } from "./controls.ts";
import { IDENTITY, type Mat3, type Vec3, norm, sub } from "./vec.ts";

const close = (a: Vec3, b: Vec3, tolerance = 1e-9) => norm(sub(a, b)) < tolerance;

/** The galactic rotation of the tile manifests (Hipparcos, ESA 1997). */
const TO_GALACTIC: Mat3 = [
  [-0.05487556041621561, -0.873437090234885, -0.48383501554871317],
  [0.49410942787558354, -0.44482962996001124, 0.7469822444972188],
  [-0.8676661490190047, -0.1980763734312013, 0.45598377617506697],
];

const view: View = { focus: [0, 0, 0], distance: 10, yaw: 0, pitch: 0 };

describe("the view", () => {
  it("puts the focus at the centre of the viewport, at its distance", () => {
    const at = project({ ...view, focus: [100, 5, -3] }, TO_GALACTIC, [100, 5, -3], 800, 600);
    expect(at?.x).toBeCloseTo(400);
    expect(at?.y).toBeCloseTo(300);
    expect(at?.depth).toBeCloseTo(10);
  });

  it("places the eye at its distance from the focus", () => {
    const tilted = { ...view, yaw: 1, pitch: 0.4, focus: [3, 4, 5] as Vec3 };
    expect(norm(sub(eyeOf(tilted, TO_GALACTIC), tilted.focus))).toBeCloseTo(10);
  });

  it("keeps the frame's pole up on the screen", () => {
    // The frame's pole in the ICRS: the last row of the rotation into the frame.
    const pole: Vec3 = TO_GALACTIC[2];
    const above = project(view, TO_GALACTIC, pole, 800, 600);
    expect(above?.x).toBeCloseTo(400);
    expect(above?.y ?? 300).toBeLessThan(300);
  });

  it("leaves a point behind the eye undrawn", () => {
    const eye = eyeOf(view, IDENTITY);
    expect(project(view, IDENTITY, [eye[0] * 2, eye[1] * 2, eye[2] * 2], 800, 600)).toBeUndefined();
  });

  it("turns about the focus and stops short of either pole", () => {
    const turned = turn(view, 100, 0);
    expect(close(turned.focus, view.focus)).toBe(true);
    expect(turned.yaw).not.toBe(view.yaw);
    expect(turn(view, 0, 1e6).pitch).toBe(MAX_PITCH);
    expect(turn(view, 0, -1e6).pitch).toBe(-MAX_PITCH);
    expect(MAX_PITCH).toBeLessThan(Math.PI / 2);
  });
});
