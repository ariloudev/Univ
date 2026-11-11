import { describe, expect, it } from "vitest";

import { KIND_COLOURS } from "./kinds.ts";
import { NO_COLOUR_GREY, blackBodyColour } from "./starColour.ts";
import { POINT_SIZE, TRACERS_DOMAIN, TRACER_COLOURS, TRACER_KINDS, TRACER_OBJECTS, objectIndex, pointSizeOf, tracerBytes, tracerColour } from "./tracers.ts";

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
