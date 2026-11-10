import { describe, expect, it } from "vitest";

import { KIND_COLOURS } from "./kinds.ts";
import { NO_COLOUR_GREY, blackBodyColour } from "./starColour.ts";
import { POINT_SIZE, TRACERS_DOMAIN, TRACER_COLOURS, pointSizeOf, tracerBytes, tracerColour } from "./tracers.ts";

const distance = (a: readonly number[], b: readonly number[]) => Math.hypot(...[0, 1, 2].map((i) => (a[i] ?? 0) - (b[i] ?? 0)));

describe("the Galaxy's tracers", () => {
  it("gives each source a colour apart from every star temperature's, the stars' grey, the galaxies' blue and each other", () => {
    const stars = Array.from({ length: 77 }, (_, i) => blackBodyColour(2000 + i * 500));
    const others = [...stars, NO_COLOUR_GREY, ...KIND_COLOURS];
    for (const [i, colour] of TRACER_COLOURS.entries()) {
      for (const other of others) expect(distance(colour, other)).toBeGreaterThan(0.3);
      for (const next of TRACER_COLOURS.slice(i + 1)) expect(distance(colour, next)).toBeGreaterThan(0.3);
    }
  });

  it("colours each point by its source, from the first colour again past the last", () => {
    const bytes = tracerBytes(Uint16Array.from([0, 1, TRACER_COLOURS.length]));
    expect([...bytes.slice(0, 4)]).toEqual([...bytes.slice(8, 12)]);
    expect([...bytes.slice(0, 4)]).not.toEqual([...bytes.slice(4, 8)]);
    expect(tracerColour(TRACER_COLOURS.length + 1)).toBe(tracerColour(1));
  });

  it("draws a tracer twice the size of a star, and every other point a star's size", () => {
    expect(pointSizeOf(TRACERS_DOMAIN)).toBe(2 * pointSizeOf("stars"));
    expect(pointSizeOf("stars")).toBe(POINT_SIZE);
    expect(pointSizeOf("cosmic_web")).toBe(POINT_SIZE);
  });
});
