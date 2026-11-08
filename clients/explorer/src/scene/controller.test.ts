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
  const drawn: { lines: Map<string, Lines | undefined>; draws: number; set: string[] } = { lines: new Map(), draws: 0, set: [] };
  const drawer: Drawer = {
    draw: () => {
      drawn.draws += 1;
    },
    setLines: (name, lines) => {
      drawn.set.push(name);
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

  it("centres on a point at the distance it was seen from, and then turns about it", () => {
    const { controller, advance } = setup();
    const point: Vec3 = [120, -40, 15];
    const before = controller.view.distance;
    controller.centreOn(point);
    advance(FLIGHT_MS);
    controller.tick();
    expect(controller.flying).toBe(false);
    expect(controller.view.distance).toBe(before);
    const expectCentred = () => {
      const at = project(controller.view, controller.toFrame, point, 800, 600);
      expect(at?.x).toBeCloseTo(400);
      expect(at?.y).toBeCloseTo(300);
    };
    expectCentred();
    controller.turn(120, -40);
    expect(controller.view.focus).toEqual(point);
    expectCentred();
  });

  it("keeps the distance a flight under way ends at when centring on a point", () => {
    const { controller, advance } = setup();
    controller.flyTo([10, 0, 0], 0.5);
    advance(FLIGHT_MS / 2);
    controller.tick();
    controller.centreOn([20, 0, 0]);
    advance(FLIGHT_MS);
    controller.tick();
    expect(controller.view).toMatchObject({ focus: [20, 0, 0], distance: 0.5 });
  });

  it("does not start a flight over toward where it already is or is going", () => {
    const { controller, advance } = setup();
    controller.flyTo([10, 0, 0], 0.5);
    advance(FLIGHT_MS / 2);
    controller.tick();
    const halfway = controller.view;
    controller.centreOn([10, 0, 0]);
    controller.tick();
    expect(controller.view).toEqual(halfway);
    advance(FLIGHT_MS / 2);
    controller.tick();
    expect(controller.flying).toBe(false);
    controller.centreOn([10, 0, 0]);
    expect(controller.flying).toBe(false);
  });

  it("centres on a point at once where reduced motion is asked for", () => {
    const { controller } = setup(true);
    const before = controller.view.distance;
    controller.centreOn([1, 2, 3]);
    expect(controller.flying).toBe(false);
    expect(controller.view).toMatchObject({ focus: [1, 2, 3], distance: before });
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

  it("draws the distance rings in place of the equator's ring while they are on, and the ring again once off", () => {
    const { controller, drawn } = setup();
    controller.tick();
    expect(drawn.lines.get("equator")).toBeDefined();
    controller.setRings(true);
    controller.tick();
    expect(drawn.lines.get("equator")).toBeUndefined();
    expect(drawn.lines.get("distance-rings")?.offsets.length).toBeGreaterThan(0);
    expect(controller.rings?.step).toBeGreaterThan(0);
    // The rings fade as they crowd, by their step; the spokes, drawn apart, do not.
    expect(drawn.lines.get("distance-rings")?.fade).toBe(controller.rings?.step);
    expect(drawn.lines.get("distance-spokes")?.offsets.length).toBe(12 * 6);
    expect(drawn.lines.get("distance-spokes")?.fade).toBeUndefined();
    // Drawn again only when the rings change: a turn keeps them, a zoom past a step does not.
    const count = () => drawn.set.filter((name) => name === "distance-rings").length;
    const before = count();
    controller.turn(40, 10);
    controller.tick();
    expect(count()).toBe(before);
    controller.zoom(-8);
    controller.tick();
    expect(count()).toBe(before + 1);
    // The axes change them: the old rings go at once, and the new ones are drawn.
    expect(controller.rings?.axes).toBe("galactic");
    controller.setAxes("supergalactic");
    expect(drawn.lines.get("distance-rings")).toBeUndefined();
    expect(drawn.lines.get("distance-spokes")).toBeUndefined();
    controller.tick();
    expect(controller.rings?.axes).toBe("supergalactic");
    expect(drawn.lines.get("distance-rings")?.offsets.length).toBeGreaterThan(0);
    controller.setRings(false);
    controller.tick();
    expect(drawn.lines.get("distance-rings")).toBeUndefined();
    expect(drawn.lines.get("distance-spokes")).toBeUndefined();
    expect(drawn.lines.get("equator")).toBeDefined();
    expect(controller.rings).toBeUndefined();
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
