import { describe, expect, it } from "vitest";

import { MAX_PITCH, type View, eyeAxes, eyeOf, pixelsAt, project } from "./camera.ts";
import { turn } from "./controls.ts";
import { manifestOf } from "../test/fixtures.ts";
import { IDENTITY, type Mat3, type Vec3, add, norm, normalize, scale, sub } from "./vec.ts";

const close = (a: Vec3, b: Vec3, tolerance = 1e-9) => norm(sub(a, b)) < tolerance;

/** The galactic rotation of the tile manifests (Hipparcos, ESA 1997). */
const TO_GALACTIC: Mat3 = manifestOf("stars").rotations.to_galactic;

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

describe("a symbol drawn at a fixed size", () => {
  it("spans the same pixels across the view at any depth, in the middle of the view or off it", () => {
    const view: View = { focus: [0, 0, 0], distance: 1, yaw: 0.3, pitch: 0.2 };
    const [right] = eyeAxes(view, IDENTITY);
    const eye = eyeOf(view, IDENTITY);
    const ahead = normalize(sub(view.focus, eye));
    for (const distance of [0.01, 50, 3000]) {
      for (const aside of [0, 0.6]) {
        // A centre `distance` ahead, and `aside` of that across the view, where perspective
        // divides by its depth rather than its distance.
        const centre = add(add(eye, scale(ahead, distance)), scale(right, aside * distance));
        const a = project(view, IDENTITY, centre, 800, 600);
        expect(a).toBeDefined();
        if (a === undefined) continue;
        const b = project(view, IDENTITY, add(centre, scale(right, pixelsAt(12, a.depth, 600))), 800, 600);
        expect(b).toBeDefined();
        expect((b?.x ?? NaN) - a.x).toBeCloseTo(12, 6);
      }
    }
  });
});
