import { describe, expect, it } from "vitest";

import { KIND_COLOURS, POINT_OPACITY } from "./kinds.ts";
import { NO_COLOUR_GREY, blackBodyColour } from "./starColour.ts";
import { POINT_SIZE, TRACERS_DOMAIN, TRACER_COLOURS, TRACER_KINDS, TRACER_OBJECTS, objectIndex, pointSizeOf, tracerBytes, tracerColour, tracerColoursOf } from "./tracers.ts";

const distance = (a: readonly number[], b: readonly number[]) => Math.hypot(...[0, 1, 2].map((i) => (a[i] ?? 0) - (b[i] ?? 0)));

describe("the Galaxy's tracers", () => {
  it("gives each kind of object a colour apart from every star temperature's, the stars' grey, the galaxies' blue and each other", () => {
    const stars = Array.from({ length: 77 }, (_, i) => blackBodyColour(2000 + i * 500));
    const others = [...stars, NO_COLOUR_GREY, ...KIND_COLOURS];
    for (const [i, colour] of TRACER_COLOURS.entries()) {
      for (const other of others) expect(distance(colour, other)).toBeGreaterThan(0.3);
      for (const next of TRACER_COLOURS.slice(i + 1)) expect(distance(colour, next)).toBeGreaterThan(0.3);
    }
  });

  it("draws the luminous hot stars dark, at a tenth of the other kinds' opacity", () => {
    const hot = objectIndex("luminous_hot_star");
    for (const [i, colour] of TRACER_COLOURS.entries()) if (i !== hot) expect(colour[3]).toBe(POINT_OPACITY);
    const colour = tracerColour(hot);
    expect(colour[3]).toBeCloseTo(0.1 * POINT_OPACITY);
    // Dark: no channel above half, where a star's colour or a galaxy's blue reaches full.
    expect(Math.max(colour[0], colour[1], colour[2])).toBeLessThanOrEqual(0.5);
  });

  it("draws the RR Lyrae stars fainter where more than 4 of a tile share a 64th of its edge, never below a tenth", () => {
    const rr = objectIndex("rr_lyrae_star");
    const hot = objectIndex("luminous_hot_star");
    const edge = 64;
    // 8 RR Lyrae stars in one cell, 40 in another, 1 alone, 4 sharing a cell, and 8 hot stars crowded too.
    const places: [number, [number, number, number]][] = [
      ...Array.from({ length: 8 }, (): [number, [number, number, number]] => [rr, [0.2, 0.2, 0.2]]),
      ...Array.from({ length: 40 }, (): [number, [number, number, number]] => [rr, [10.5, 10.5, 10.5]]),
      [rr, [-20.5, 5.5, 5.5]],
      ...Array.from({ length: 4 }, (): [number, [number, number, number]] => [rr, [-10.5, -10.5, -10.5]]),
      ...Array.from({ length: 8 }, (): [number, [number, number, number]] => [hot, [0.2, 0.2, 0.2]]),
    ];
    const objects = Uint8Array.from(places, ([object]) => object);
    const offsets = Float32Array.from(places.flatMap(([, at]) => at));
    const bytes = tracerColoursOf(objects, offsets, edge);
    const alpha = (i: number) => bytes[i * 4 + 3];
    const full = tracerBytes(objects);
    expect(alpha(0)).toBe(Math.round((full[3] ?? 0) * 0.5));
    expect(alpha(8)).toBe(Math.round((full[3] ?? 0) * 0.1));
    expect(alpha(48)).toBe(full[48 * 4 + 3]);
    expect(alpha(49)).toBe(full[49 * 4 + 3]);
    expect(alpha(53)).toBe(full[53 * 4 + 3]);
    expect([...bytes.slice(0, 3)]).toEqual([...full.slice(0, 3)]);
  });

  it("has a colour for each kind of object of the tracers, and one for a tracer of unknown kind", () => {
    expect(TRACER_OBJECTS.map((o) => o.id)).toEqual(["unknown", ...TRACER_KINDS]);
    expect(TRACER_KINDS).toContain("luminous_hot_star");
    for (const kind of TRACER_KINDS) expect(TRACER_OBJECTS[objectIndex(kind)]?.id).toBe(kind);
  });

  it("colours each point by its kind of object, and a point of no kind or an unknown index as unknown", () => {
    const hot = objectIndex("luminous_hot_star");
    const bytes = tracerBytes(Uint8Array.from([0, hot, objectIndex("open_cluster"), 250]));
    expect([...bytes.slice(0, 4)]).toEqual([...bytes.slice(12, 16)]);
    expect([...bytes.slice(4, 8)]).not.toEqual([...bytes.slice(0, 4)]);
    expect([...bytes.slice(4, 8)]).not.toEqual([...bytes.slice(8, 12)]);
    expect(tracerColour(hot)).toBe(TRACER_COLOURS[hot]);
    expect(tracerColour(250)).toBe(tracerColour(0));
    expect(objectIndex(undefined)).toBe(0);
    expect(objectIndex("galaxy_group")).toBe(0);
  });

  it("draws a tracer twice the size of a star, and every other point a star's size", () => {
    expect(pointSizeOf(TRACERS_DOMAIN)).toBe(2 * pointSizeOf("stars"));
    expect(pointSizeOf("stars")).toBe(POINT_SIZE);
    expect(pointSizeOf("cosmic_web")).toBe(POINT_SIZE);
  });
});
