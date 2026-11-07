import { describe, expect, it } from "vitest";

import type { View } from "./camera.ts";
import { MIN_GAP, edgeTicks, lonLat, tickLabel, ticksOf } from "./graticule.ts";
import { IDENTITY, type Mat3 } from "./vec.ts";


const RAD = Math.PI / 180;

/** Looking from far out along the frame's longitude `lon` and latitude `lat`, degrees, back
 * toward the focus at the origin: the direction of sight at the centre is the opposite one. */
const view = (lon: number, lat: number, distance = 1): View => ({ focus: [0, 0, 0], distance, yaw: lon * RAD, pitch: lat * RAD });

const at = (ticks: ReturnType<typeof edgeTicks>, edge: string) => ticks.find((t) => t.edge === edge);

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

  it("choose the finer of two steps when the coarser one crowds", () => {
    // Samples over 0 to 100 degrees, 10 pixels a degree: 30 degree steps are 300 pixels apart.
    const samples = Array.from({ length: 101 }, (_, i) => [i * 10, i] as const);
    expect(ticksOf(samples, 1000, false)?.step).toBe(10);
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
