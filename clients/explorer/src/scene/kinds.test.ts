import { describe, expect, it } from "vitest";

import { KINDS, KIND_COLOURS, KIND_INDEX, cssColour, pointOpacity } from "./kinds.ts";

describe("the kinds of value", () => {
  it("are drawn in four distinct colours, in one order whatever a manifest lists", () => {
    expect(KINDS).toEqual(["measured", "estimated", "inferred", "reconstructed"]);
    expect(new Set(KINDS.map(cssColour)).size).toBe(4);
    expect(new Set(KIND_COLOURS.map((c) => c.join())).size).toBe(4);
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
