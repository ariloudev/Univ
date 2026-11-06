import { describe, expect, it } from "vitest";

import { EVERYTHING, LEVELS } from "../catalogue/lod.ts";
import { CHOSEN_COLOUR, CULLED_OPACITY, POINT_OPACITY, KINDS, KIND_COLOURS, KIND_INDEX, POINT_CSS, STRUCTURE_COLOUR, css, detailOpacity, pointOpacity, structureColour, structureCss, structureStyle } from "./kinds.ts";

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

  it("are drawn fully opaque inside a structure the view is culled to, twice as bright as the brightest without it", () => {
    expect(POINT_OPACITY * CULLED_OPACITY).toBe(1);
    for (const distance of [1, 400, 1e6]) {
      expect(CULLED_OPACITY).toBe(2 * Math.max(detailOpacity(EVERYTHING, distance), pointOpacity(distance), 1));
    }
  });

  it("fade as the eye withdraws for a domain of one level, or with level of detail off", () => {
    expect(detailOpacity(EVERYTHING, 400)).toBe(pointOpacity(400));
    expect(detailOpacity(EVERYTHING, 4000)).toBe(pointOpacity(4000));
    expect(detailOpacity(EVERYTHING, 1e9)).toBe(0.03);
  });
});

describe("a structure's style", () => {
  it("is its own colour, fainter where approximate, or the chosen colour with larger markers where chosen", () => {
    const own = structureColour("void", false);
    const approximate = structureColour("void", true);
    expect(structureStyle(own, false).colour).toEqual(own);
    expect(approximate.slice(0, 3)).toEqual(own.slice(0, 3));
    expect(approximate[3]).toBeLessThan(own[3]);
    const chosen = structureStyle(approximate, true);
    expect(chosen.colour).toEqual(CHOSEN_COLOUR);
    expect(chosen.markerSize).toBeGreaterThan(structureStyle(own, false).markerSize * 2);
  });

  it("is a colour of each kind's own, apart from the points', the chosen structure's and every other kind's", () => {
    const kinds = ["void", "repeller", "supercluster", "supercluster_complex", "basin", "overdensity", "wall", "filament", "quasar_group", "ring", "arc", "shell"];
    const colours = kinds.map((kind) => structureColour(kind, false));
    const apart = (a: readonly number[], b: readonly number[]) => Math.hypot((a[0] ?? 0) - (b[0] ?? 0), (a[1] ?? 0) - (b[1] ?? 0), (a[2] ?? 0) - (b[2] ?? 0));
    colours.forEach((a, i) => {
      expect(apart(a, KIND_COLOURS[0] ?? [])).toBeGreaterThan(0.25);
      expect(apart(a, CHOSEN_COLOUR)).toBeGreaterThan(0.25);
      colours.slice(i + 1).forEach((b) => expect(apart(a, b)).toBeGreaterThan(0.1));
    });
    // A kind with no colour of its own is drawn in the structures' colour.
    expect(structureColour("something_new", false).slice(0, 3)).toEqual(STRUCTURE_COLOUR.slice(0, 3));
    expect(structureCss("void")).toBe(css(colours[0] ?? []));
  });

  it("draws the chosen structure's outline thicker than any other's", () => {
    const own = structureColour("wall", false);
    expect(structureStyle(own, true).lineWidth).toBeGreaterThanOrEqual(3 * structureStyle(own, false).lineWidth);
  });
});
