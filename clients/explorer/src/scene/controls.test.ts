import { describe, expect, it } from "vitest";

import { MAX_DISTANCE, MIN_DISTANCE, type View, project, unitsPerPixel } from "./camera.ts";
import { CLICK_SLOP, STEP_FACTOR, between, isClick, pan, viewingDistance, wheelSteps, zoom } from "./controls.ts";
import { IDENTITY, type Vec3, norm, sub } from "./vec.ts";

const view: View = { focus: [10, 0, 0], distance: 1, yaw: 0.3, pitch: 0.2 };

describe("the mouse", () => {
  it("changes the distance by the same factor for each step of the wheel", () => {
    const one = zoom(view, 1);
    const two = zoom(one, 1);
    expect(view.distance / one.distance).toBeCloseTo(STEP_FACTOR);
    expect(one.distance / two.distance).toBeCloseTo(STEP_FACTOR);
    expect(zoom(view, -1).distance / view.distance).toBeCloseTo(STEP_FACTOR);
  });

  it("reads a notch of the wheel as one step, in pixels, lines or pages", () => {
    expect(wheelSteps(100, 0)).toBe(1);
    expect(wheelSteps(3, 1)).toBeCloseTo(0.99);
    expect(wheelSteps(-100, 0)).toBe(-1);
  });

  it("keeps the eye between 0.1 parsec and 20 gigaparsecs from the focus", () => {
    expect(MIN_DISTANCE).toBe(0.1e-6);
    expect(MAX_DISTANCE).toBe(20e3);
    expect(zoom(view, 1e4).distance).toBe(MIN_DISTANCE);
    expect(zoom(view, -1e4).distance).toBe(MAX_DISTANCE);
  });

  it("moves the focus in the plane facing the eye by the distance dragged at its depth", () => {
    const moved = pan(view, IDENTITY, 50, 0, 600);
    const shift = norm(sub(moved.focus, view.focus));
    expect(shift).toBeCloseTo(50 * unitsPerPixel(view, 600));
    // The old focus now lies 50 pixels to the right of the centre.
    const old = project(moved, IDENTITY, view.focus, 800, 600);
    expect(old?.x).toBeCloseTo(450);
    expect(old?.y).toBeCloseTo(300);
  });

  it("calls a press that moves less than 6 pixels a click", () => {
    expect(CLICK_SLOP).toBe(6);
    expect(isClick(5.9)).toBe(true);
    expect(isClick(6)).toBe(false);
  });

  it("flies by easing the focus and scaling the distance, ending where it was sent", () => {
    const to: View = { ...view, focus: [0, 0, 0] as Vec3, distance: 100 };
    expect(between(view, to, 0)).toEqual({ ...to, focus: view.focus, distance: view.distance });
    const end = between(view, to, 1);
    expect(end.focus).toEqual([0, 0, 0]);
    expect(end.distance).toBeCloseTo(100);
    expect(between(view, to, 0.5).distance).toBeCloseTo(10);
  });

  it("turns the eye as it flies, the short way round, toward the direction it was sent", () => {
    const from: View = { ...view, yaw: 3, pitch: 0.2 };
    const to: View = { ...view, yaw: -3, pitch: Math.PI / 2 };
    // Landing exactly where it was sent, so that a second flight there sees it already there.
    expect(between(from, to, 1)).toBe(to);
    // Halfway: the yaw past ±π, not back through 0.
    const half = between(from, to, 0.5);
    expect(Math.abs(Math.cos(half.yaw) + 1)).toBeLessThan(0.01);
    expect(half.pitch).toBeCloseTo((0.2 + Math.PI / 2) / 2);
  });

  it("views a thing from a twentieth of its distance, or from its size", () => {
    expect(viewingDistance(100)).toBeCloseTo(5);
    expect(viewingDistance(1e-5)).toBe(1e-6);
    expect(viewingDistance(100, 20)).toBe(50);
  });
});
