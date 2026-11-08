import { describe, expect, it } from "vitest";

import type { View } from "./camera.ts";
import { type Edge, type EdgeTicks, MIN_GAP, edgeTicks, fitting, lonLat, stepsFor, tickLabel } from "./graticule.ts";
import { IDENTITY, type Mat3 } from "./vec.ts";


const RAD = Math.PI / 180;

/** Looking from far out along the frame's longitude `lon` and latitude `lat`, degrees, back
 * toward the focus at the origin: the direction of sight at the centre is the opposite one. */
const view = (lon: number, lat: number, distance = 1): View => ({ focus: [0, 0, 0], distance, yaw: lon * RAD, pitch: lat * RAD });

const at = (ticks: ReturnType<typeof edgeTicks>, edge: string) => ticks.find((t) => t.edge === edge);

/** The ticks of each edge, by edge. */
const byEdge = (ticks: EdgeTicks[]) => Object.fromEntries(ticks.map((t) => [t.edge, t])) as Record<Edge, EdgeTicks>;

describe("the ticks on the view's edges", () => {
  it("name each edge's coordinate in the chosen axes", () => {
    const names = edgeTicks(view(0, 0), IDENTITY, "galactic", 1280, 800).map((t) => [t.edge, t.name]);
    expect(names).toEqual([
      ["top", "galactic longitude"],
      ["bottom", "galactic longitude"],
      ["left", "galactic latitude"],
      ["right", "galactic latitude"],
    ]);
    expect(edgeTicks(view(0, 0), IDENTITY, "equatorial", 1280, 800).map((t) => t.name)).toContain("declination");
  });

  it("cross round longitudes and latitudes where the edge's direction of sight does", () => {
    // The eye on the +x axis looks along -x: longitude 180 at the centre, latitude 0.
    const ticks = edgeTicks(view(0, 0), IDENTITY, "galactic", 1280, 800);
    const top = at(ticks, "top");
    const middle = top?.ticks.find((t) => t.value === 180);
    expect(middle?.at).toBeCloseTo(640, 0);
    const left = at(ticks, "left");
    expect(left?.ticks.find((t) => t.value === 0)?.at).toBeCloseTo(400, 0);
    // Longitude grows to the left as the sky is seen from inside: east is left.
    const values = (top?.ticks ?? []).toSorted((a, b) => a.at - b.at).map((t) => t.value);
    expect(values[0]).toBeGreaterThan(values[values.length - 1] ?? 0);
  });

  it("use the finest step whose labels stay at least 60 pixels apart", () => {
    for (const [width, height] of [
      [1280, 800],
      [640, 400],
      [320, 900],
    ] as const) {
      for (const edge of edgeTicks(view(30, 20), IDENTITY, "supergalactic", width, height)) {
        const along = edge.ticks.map((t) => t.at).sort((a, b) => a - b);
        for (let i = 1; i < along.length; i += 1) expect((along[i] ?? 0) - (along[i - 1] ?? 0)).toBeGreaterThanOrEqual(MIN_GAP);
      }
    }
    // A 50 degree field across 800 pixels: 5 degrees are about 90 pixels, 2 degrees 36.
    const left = at(edgeTicks(view(0, 0), IDENTITY, "galactic", 1280, 800), "left");
    const values = (left?.ticks ?? []).map((t) => t.value).sort((a, b) => a - b);
    expect(values).toEqual([-20, -15, -10, -5, 0, 5, 10, 15, 20]);
  });

  it("space each edge at a multiple of the finer step of its coordinate's two edges", () => {
    const steps = [90, 30, 15, 10, 5, 2, 1];
    const fit = (...fitted: [number, number][]) => new Map(fitted.map(([step, ticks]) => [step, Array.from({ length: ticks }, (_, i) => ({ at: i * 100, value: i * step }))]));
    // 10 fits one edge and 15 the other: the finer sets the lines, the other takes 30.
    expect(stepsFor([fit([90, 1], [30, 3], [15, 5], [10, 8]), fit([90, 1], [30, 3], [15, 6])], steps)).toEqual({ line: 10, edges: [10, 30] });
    // An edge that only fits 90 near a pole leaves the other its fine step.
    expect(stepsFor([fit([90, 2]), fit([90, 1], [30, 3], [10, 8], [5, 16])], steps)).toEqual({ line: 5, edges: [90, 5] });
    // A fit with no tick sets nothing; an edge with no fit takes none.
    expect(stepsFor([fit([90, 0], [30, 0]), fit([90, 1], [5, 16])], steps)).toEqual({ line: 5, edges: [30, 5] });
    expect(stepsFor([new Map(), fit([90, 1], [5, 16])], steps)).toEqual({ line: 5, edges: [undefined, 5] });
    expect(stepsFor([new Map(), new Map()], steps)).toEqual({ line: undefined, edges: [undefined, undefined] });
  });

  it("keep the labels of an edge far from the pole where the other edge passes near it", () => {
    // Views where one edge passes near a pole and crowds: the other keeps its own fine ticks.
    for (const [lon, lat, width, height] of [
      [33.3, -60, 1280, 800],
      [0.3, -64.7, 1280, 800],
      [0, -45, 1280, 800],
    ] as const) {
      const { top, bottom, left, right } = byEdge(edgeTicks(view(lon, lat), IDENTITY, "galactic", width, height));
      expect(bottom.ticks.length, `${lon}, ${lat}`).toBeGreaterThan(8);
      for (const edge of [top, bottom]) {
        expect(edge.lineStep).toBe(bottom.step);
        for (const t of edge.ticks) expect(Math.abs(t.value / (edge.lineStep ?? 1) - Math.round(t.value / (edge.lineStep ?? 1)))).toBeLessThan(1e-6);
      }
      expect(right.step).toBe(left.step);
    }
  });

  it("count a round value met exactly at a sample once", () => {
    // The bottom edge's middle sample lies exactly on 240 degrees.
    const { top, bottom } = byEdge(edgeTicks(view(60, -14), IDENTITY, "galactic", 800, 600));
    expect(bottom.ticks.length).toBeGreaterThan(3);
    expect(top.ticks.length).toBeGreaterThan(3);
    expect(bottom.ticks.filter((t) => t.value === 240)).toHaveLength(1);
  });

  it("choose the finer of two steps when the coarser one crowds", () => {
    // Samples over 0 to 100 degrees, 10 pixels a degree: 30 degree steps are 300 pixels apart.
    const samples = Array.from({ length: 101 }, (_, i) => [i * 10, i] as const);
    const steps = [90, 30, 15, 10, 5, 2, 1];
    expect(stepsFor([fitting(samples, 1000, false, steps)], steps)).toEqual({ line: 10, edges: [10] });
  });

  it("step right ascension by round hours and minutes", () => {
    const top = at(edgeTicks(view(0, 0), IDENTITY, "equatorial", 1280, 800), "top");
    const labels = top?.ticks.map((t) => t.label) ?? [];
    expect(labels.length).toBeGreaterThan(3);
    for (const label of labels) expect(label).toMatch(/^\d+h( [1-5]0m)?$/);
  });

  it("label right ascension in hours and minutes, and every other coordinate in degrees", () => {
    expect(tickLabel(300, "longitude", "equatorial")).toBe("20h");
    expect(tickLabel(65, "longitude", "equatorial")).toBe("4h 20m");
    expect(tickLabel(0.1, "longitude", "equatorial")).toBe("0h 0.4m");
    expect(tickLabel(360, "longitude", "galactic")).toBe("0°");
    expect(tickLabel(12.5, "longitude", "supergalactic")).toBe("12.5°");
    expect(tickLabel(30, "latitude", "equatorial")).toBe("+30°");
    expect(tickLabel(-15, "latitude", "galactic")).toBe("−15°");
    expect(tickLabel(0, "latitude", "galactic")).toBe("0°");
  });

  it("follow the view as it turns and as the axes change, wherever the eye is", () => {
    const top = (v: View, toFrame: Mat3 = IDENTITY) => at(edgeTicks(v, toFrame, "galactic", 1280, 800), "top")?.ticks;
    expect(top(view(10, 0))).not.toEqual(top(view(0, 0)));
    // The eye's direction is kept in the chosen axes, so other axes name and label the edges anew.
    const equatorial = at(edgeTicks(view(0, 0), IDENTITY, "equatorial", 1280, 800), "top");
    expect(equatorial?.name).toBe("right ascension");
    expect(equatorial?.ticks.map((t) => t.label)).toContain("12h");
    // The same direction of sight from an eye far away, or with the focus elsewhere: the same ticks.
    expect(top({ ...view(0, 0), distance: 5000 })).toEqual(top(view(0, 0)));
    expect(top({ ...view(0, 0), focus: [100, -40, 7] })).toEqual(top(view(0, 0)));
  });

  it("give longitude and latitude of a direction", () => {
    expect(lonLat([0, -1, 0])).toEqual({ lon: 270, lat: 0 });
    expect(lonLat([0, 0, 1]).lat).toBe(90);
  });
});
