import { describe, expect, it } from "vitest";

import { manifestOf } from "../test/fixtures.ts";
import { eyeOf, project } from "./camera.ts";
import { Controller } from "./controller.ts";
import { FLIGHT_MS } from "./controls.ts";
import type { Drawer, Lines } from "./drawing.ts";
import { IDENTITY, type Vec3, apply, norm, sub } from "./vec.ts";

// The rotations a tile manifest gives (the decoder's fixture of the stars domain).
const ROTATIONS = manifestOf("stars").rotations;
const TO_SUPERGALACTIC = ROTATIONS.to_supergalactic;

function setup(reduced = false) {
  const drawn: { lines: Map<string, Lines | undefined>; draws: number } = { lines: new Map(), draws: 0 };
  const drawer: Drawer = {
    draw: () => {
      drawn.draws += 1;
    },
    setLines: (name, lines) => {
      drawn.lines.set(name, lines);
    },
  };
  let clock = 0;
  const controller = new Controller(drawer, {
    size: () => ({ width: 800, height: 600, ratio: 1 }),
    now: () => clock,
    reducedMotion: () => reduced,
  });
  controller.setRotations(ROTATIONS);
  return { controller, drawn, advance: (ms: number) => (clock += ms) };
}

describe("the controller", () => {
  it("flies to a thing within the flight's time and ends with it at the centre", () => {
    const { controller, advance } = setup();
    const target: Vec3 = [120, -40, 15];
    controller.flyTo(target, 5);
    controller.tick();
    expect(controller.flying).toBe(true);
    advance(FLIGHT_MS);
    controller.tick();
    expect(controller.flying).toBe(false);
    expect(FLIGHT_MS).toBeLessThanOrEqual(1500);
    const at = project(controller.view, controller.toFrame, target, 800, 600);
    expect(at?.x).toBeCloseTo(400);
    expect(at?.y).toBeCloseTo(300);
  });

  it("moves at once where reduced motion is asked for", () => {
    const { controller } = setup(true);
    controller.flyTo([1, 2, 3], 0.5);
    expect(controller.flying).toBe(false);
    expect(controller.view.focus).toEqual([1, 2, 3]);
  });

  it("returns the focus to Earth", () => {
    const { controller, advance } = setup();
    controller.flyTo([50, 0, 0], 2);
    advance(FLIGHT_MS);
    controller.tick();
    controller.returnToEarth();
    advance(FLIGHT_MS);
    controller.tick();
    expect(norm(controller.view.focus)).toBe(0);
  });

  it("draws the chosen frame's equator as a ring around Earth", () => {
    const { controller, drawn } = setup();
    controller.setAxes("supergalactic");
    controller.tick();
    const ring = drawn.lines.get("equator");
    expect(ring?.centre).toEqual([0, 0, 0]);
    const first: Vec3 = [ring?.offsets[0] ?? 0, ring?.offsets[1] ?? 0, ring?.offsets[2] ?? 0];
    // In supergalactic axes the ring's points lie in the plane z = 0, at its radius.
    const inFrame = apply(TO_SUPERGALACTIC, first);
    // Float32 offsets: within a millionth of the radius.
    expect(Math.abs(inFrame[2]) / controller.ringRadius).toBeLessThan(1e-6);
    expect(Math.abs(norm(first) / controller.ringRadius - 1)).toBeLessThan(1e-6);
  });

  it("changes the axes without moving the focus or any point", () => {
    const { controller } = setup();
    const point: Vec3 = [3, -7, 11];
    const before = norm(sub(point, eyeOf(controller.view, controller.toFrame)));
    const focus = controller.view.focus;
    controller.setAxes("equatorial");
    expect(controller.view.focus).toBe(focus);
    // The eye keeps its distance from the focus; the point keeps its place in the ICRS.
    expect(norm(sub(eyeOf(controller.view, controller.toFrame), focus))).toBeCloseTo(controller.view.distance);
    expect(point).toEqual([3, -7, 11]);
    expect(before).toBeGreaterThan(0);
    expect(controller.toFrame).toEqual(IDENTITY);
  });

  it("draws only when something changed", () => {
    const { controller, drawn } = setup();
    expect(controller.tick()).toBe(true);
    expect(controller.tick()).toBe(false);
    expect(drawn.draws).toBe(1);
    controller.turn(10, 0);
    controller.tick();
    expect(drawn.draws).toBe(2);
  });

  it("shows the ICRS's own axes until the release's rotations arrive, then galactic", () => {
    const drawer: Drawer = { draw: () => undefined, setLines: () => undefined };
    const controller = new Controller(drawer, {
      size: () => ({ width: 800, height: 600, ratio: 1 }),
      now: () => 0,
      reducedMotion: () => false,
    });
    expect(controller.axes).toBe("equatorial");
    expect(controller.canShow("galactic")).toBe(false);
    controller.setAxes("supergalactic");
    expect(controller.axes).toBe("equatorial");
    controller.setRotations(ROTATIONS);
    expect(controller.axes).toBe("galactic");
    expect(controller.frame.basis).toContain("Hipparcos");
  });
});
