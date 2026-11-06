import { describe, expect, it } from "vitest";

import { EVERYTHING, LEVELS } from "../catalogue/lod.ts";
import { APPROXIMATE_COLOUR, CHOSEN_COLOUR, KINDS, KIND_COLOURS, KIND_INDEX, POINT_CSS, STRUCTURE_COLOUR, css, detailOpacity, pointOpacity, structureStyle } from "./kinds.ts";

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

  it("are drawn as opaque while level of detail still thins their domain as the eye withdraws, and fade beyond", () => {
    // The view starts to need the level below the root within 58,677 Mpc, as for release 10's
    // cosmic web: up to there the points are fully opaque, beyond it they fade.
    const onsets = new Float32Array(LEVELS);
    onsets[1] = 58_677;
    const thinned = { onsets, cumulative: new Float32Array(LEVELS).fill(1) };
    expect(detailOpacity(thinned, 400)).toBe(1);
    expect(detailOpacity(thinned, 20_000)).toBe(1);
    expect(detailOpacity(thinned, 117_354)).toBeCloseTo(0.5);
    expect(detailOpacity(thinned, 117_354)).toBeGreaterThan(pointOpacity(117_354));
  });

  it("fade as the eye withdraws for a domain of one level, or with level of detail off", () => {
    expect(detailOpacity(EVERYTHING, 400)).toBe(pointOpacity(400));
    expect(detailOpacity(EVERYTHING, 4000)).toBe(pointOpacity(4000));
    expect(detailOpacity(EVERYTHING, 1e9)).toBe(0.03);
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
