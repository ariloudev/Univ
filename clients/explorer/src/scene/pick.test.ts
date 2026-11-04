import { describe, expect, it } from "vitest";

import { type View, eyeOf, project } from "./camera.ts";
import { PICK_SLOP, type Pickable, pick } from "./pick.ts";
import { IDENTITY, type Vec3 } from "./vec.ts";

const view: View = { focus: [0, 0, 0], distance: 10, yaw: 0.4, pitch: 0.2 };
const SIZE = { width: 800, height: 600 };

function batch(centre: Vec3, points: Vec3[]): Pickable {
  const offsets = new Float32Array(points.flatMap((p) => [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]]));
  return { centre, radius: 5, offsets };
}

const at = (p: Vec3) => {
  const on = project(view, IDENTITY, p, SIZE.width, SIZE.height);
  if (on === undefined) throw new Error("behind the eye");
  return on;
};

describe("picking", () => {
  it("finds the point under the pointer, across batches", () => {
    const batches = [batch([0, 0, 0], [[1, 0, 0], [0, 1, 0]]), batch([3, 3, 0], [[3, 3, 0], [2, 3, 1]])];
    const target = at([2, 3, 1]);
    expect(pick(batches, view, IDENTITY, { ...SIZE, x: target.x + 2, y: target.y - 1 })).toEqual({ batch: 1, index: 1 });
  });

  it("finds nothing farther than the slop from every point", () => {
    const target = at([1, 0, 0]);
    const batches = [batch([0, 0, 0], [[1, 0, 0]])];
    expect(pick(batches, view, IDENTITY, { ...SIZE, x: target.x + PICK_SLOP + 1, y: target.y })).toBeUndefined();
  });

  it("prefers the nearer of two points under the pointer", () => {
    // Two points on one line of sight from the eye, the nearer listed second.
    const eye = eyeOf(view, IDENTITY);
    const toward = (k: number): Vec3 => [eye[0] * (1 - k) + 1 * k, eye[1] * (1 - k) + 1 * k, eye[2] * (1 - k)];
    const far = toward(1.5);
    const near = toward(0.5);
    const target = at(near);
    expect(at(far).x).toBeCloseTo(target.x, 6);
    const batches = [batch([0, 0, 0], [far, near])];
    expect(pick(batches, view, IDENTITY, { ...SIZE, x: target.x, y: target.y })).toEqual({ batch: 0, index: 1 });
  });
});
