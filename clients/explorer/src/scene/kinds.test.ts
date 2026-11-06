import { describe, expect, it } from "vitest";

import { APPROXIMATE_COLOUR, CHOSEN_COLOUR, KINDS, KIND_COLOURS, KIND_INDEX, POINT_CSS, STRUCTURE_COLOUR, css, pointOpacity, structureStyle } from "./kinds.ts";

describe("the kinds of value", () => {
  it("are drawn in one colour, in one order whatever a manifest lists", () => {
    expect(KINDS).toEqual(["measured", "estimated", "inferred", "reconstructed"]);
    expect(KIND_COLOURS).toHaveLength(KINDS.length);
    expect(new Set(KIND_COLOURS.map((c) => c.join())).size).toBe(1);
    expect(KIND_COLOURS.map(css)).toEqual(KINDS.map(() => POINT_CSS));
    KINDS.forEach((kind, i) => {
      expect(KIND_INDEX[kind]).toBe(i);
    });
  });

  it("are drawn fainter as the eye withdraws beyond a gigaparsec, never vanishing", () => {
    expect(pointOpacity(400)).toBe(1);
    expect(pointOpacity(4000)).toBeCloseTo(0.25);
    expect(pointOpacity(1e9)).toBe(0.03);
  });
});

describe("a structure's style", () => {
  it("is its own colour, fainter where approximate, or the chosen colour with larger markers where chosen", () => {
    expect(structureStyle(STRUCTURE_COLOUR, false).colour).toEqual(STRUCTURE_COLOUR);
    expect(structureStyle(APPROXIMATE_COLOUR, false).colour).toEqual(APPROXIMATE_COLOUR);
    const chosen = structureStyle(APPROXIMATE_COLOUR, true);
    expect(chosen.colour).toEqual(CHOSEN_COLOUR);
    expect(chosen.markerSize).toBeGreaterThan(structureStyle(STRUCTURE_COLOUR, false).markerSize * 2);
  });
});
