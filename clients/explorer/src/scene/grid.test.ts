import { describe, expect, it } from "vitest";

import type { View } from "./camera.ts";
import type { Axes } from "./frames.ts";
import { edgePoint, edgeTicks } from "./graticule.ts";
import { type GridLine, gridLines, isEquator } from "./grid.ts";
import { IDENTITY, type Mat3 } from "./vec.ts";

const RAD = Math.PI / 180;

/** Looking from far out along the frame's longitude `lon` and latitude `lat`, degrees, back
 * toward the focus at the origin. */
const view = (lon: number, lat: number): View => ({ focus: [0, 0, 0], distance: 1, yaw: lon * RAD, pitch: lat * RAD });

/** A rotation about the z axis by `degrees`, standing in for another frame's axes. */
const turned = (degrees: number): Mat3 => {
  const c = Math.cos(degrees * RAD);
  const s = Math.sin(degrees * RAD);
  return [
    [c, s, 0],
    [-s, c, 0],
    [0, 0, 1],
  ];
};

const grid = (v: View, toFrame: Mat3 = IDENTITY, axes: Axes = "galactic", width = 1280, height = 800) =>
  gridLines(v, toFrame, width, height, edgeTicks(v, toFrame, axes, width, height));

/** The least distance from the pixel `p` to any run of `line`. */
function distanceTo(line: GridLine, [px, py]: readonly [number, number]): number {
  let least = Number.POSITIVE_INFINITY;
  for (const run of line.runs) {
    for (let i = 1; i < run.length; i += 1) {
      const [ax, ay] = run[i - 1] ?? [0, 0];
      const [bx, by] = run[i] ?? [0, 0];
      const [dx, dy] = [bx - ax, by - ay];
      const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy || 1)));
      least = Math.min(least, Math.hypot(px - (ax + t * dx), py - (ay + t * dy)));
    }
  }
  return least;
}

describe("the grid across the view", () => {
  it("draws a line through every tick on the view's edges", () => {
    const cases: [View, Mat3, Axes, number, number][] = [
      [view(0, 0), IDENTITY, "galactic", 1280, 800],
      [view(30, 20), turned(40), "supergalactic", 1280, 800],
      [view(200, -45), IDENTITY, "equatorial", 1280, 800],
      [view(100, 70), IDENTITY, "galactic", 640, 900],
      [view(0, -55), IDENTITY, "galactic", 1600, 900],
      // Ticks of 30 and 330 degrees at 82 degrees of latitude: their lines run on past 80 to them.
      [view(0, -72), IDENTITY, "galactic", 1280, 800],
    ];
    for (const [v, toFrame, axes, width, height] of cases) {
      const edges = edgeTicks(v, toFrame, axes, width, height);
      const lines = gridLines(v, toFrame, width, height, edges);
      let ticks = 0;
      for (const edge of edges) {
        for (const tick of edge.ticks) {
          const line = lines.find((l) => l.coordinate === edge.coordinate && l.value === tick.value);
          expect(line, `${edge.coordinate} ${tick.value} on the ${edge.edge} edge`).toBeDefined();
          if (line !== undefined) expect(distanceTo(line, edgePoint(edge.edge, tick.at, 0, width, height))).toBeLessThan(1);
          ticks += 1;
        }
      }
      expect(ticks).toBeGreaterThan(8);
    }
  });

  it("runs every line on past the view's edges, with no gap inside it", () => {
    for (const line of grid(view(0, 10))) {
      for (const run of line.runs) {
        const ends: (readonly [number, number])[] = [run[0] ?? [0, 0], run[run.length - 1] ?? [0, 0]];
        for (const [x, y] of ends) expect(x < 0 || x > 1280 || y < 0 || y > 800).toBe(true);
      }
    }
  });

  it("stays still while the eye moves or zooms, and turns with the view and the axes", () => {
    const still = grid(view(20, 10));
    expect(grid({ ...view(20, 10), distance: 5000 })).toEqual(still);
    expect(grid({ ...view(20, 10), focus: [100, -40, 7] })).toEqual(still);
    expect(grid(view(25, 10))).not.toEqual(still);
    expect(grid(view(20, 10), turned(30))).not.toEqual(still);
  });

  it("stops a line of longitude at 80 degrees unless it lies at a multiple of 90 degrees", () => {
    // Looking up at the north pole from below it: the pole is in view.
    const lines = grid(view(0, -80));
    const meridians = lines.filter((l) => l.coordinate === "longitude");
    expect(meridians.length).toBeGreaterThan(4);
    const pole = lines.find((l) => l.coordinate === "longitude" && l.value === 90);
    const other = meridians.find((l) => l.value % 90 !== 0);
    expect(pole).toBeDefined();
    expect(other).toBeDefined();
    // The pole lies at the centre's top, where the lines at multiples of 90 degrees meet.
    const [px, py] = [640, 400 - (800 / 2 / Math.tan(25 * RAD)) * Math.tan(10 * RAD)];
    if (pole !== undefined) expect(distanceTo(pole, [px, py])).toBeLessThan(1);
    if (other !== undefined) expect(distanceTo(other, [px, py])).toBeGreaterThan(5);
  });

  it("marks the axes' equator alone", () => {
    const lines = grid(view(0, 0));
    expect(lines.filter(isEquator).map((l) => [l.coordinate, l.value])).toEqual([["latitude", 0]]);
    expect(grid(view(0, 60)).some(isEquator)).toBe(false);
  });

  it("steps right ascension by the hours of its ticks", () => {
    const meridians = grid(view(0, 0), IDENTITY, "equatorial").filter((l) => l.coordinate === "longitude");
    for (const m of meridians) expect((m.value / 15) * 60).toBeCloseTo(Math.round((m.value / 15) * 60), 6);
  });
});
