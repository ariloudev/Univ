// @vitest-environment node
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { eyeOf, project } from "./camera.ts";
import { Controller, type Drawer } from "./controller.ts";
import { FLIGHT_MS } from "./controls.ts";
import type { Rotations } from "./frames.ts";
import type { Lines } from "./renderer.ts";
import { IDENTITY, type Vec3, apply, norm, sub } from "./vec.ts";

// The rotations a tile manifest gives (the decoder's fixture of the stars domain).
const ROTATIONS = (
  JSON.parse(
    readFileSync(new URL("../../../tile-decoder/fixtures/stars.manifest.json", import.meta.url), "utf8"),
  ) as { rotations: Rotations }
).rotations;
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
    controller.frame();
    expect(controller.flying).toBe(true);
    advance(FLIGHT_MS);
    controller.frame();
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
    controller.frame();
    controller.returnToEarth();
    advance(FLIGHT_MS);
    controller.frame();
    expect(norm(controller.view.focus)).toBe(0);
  });

  it("draws the chosen frame's equator as a ring around Earth", () => {
    const { controller, drawn } = setup();
    controller.setAxes("supergalactic");
    controller.frame();
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
    controller.frame();
    controller.frame();
    expect(drawn.draws).toBe(1);
    controller.turn(10, 0);
    controller.frame();
    expect(drawn.draws).toBe(2);
  });
});
