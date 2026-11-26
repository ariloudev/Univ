import { describe, expect, it } from "vitest";

import { EVERYTHING } from "../catalogue/lod.ts";
import { detailOpacity } from "./kinds.ts";
import { domainOpacity, starsFade } from "./starsFade.ts";

const KPC = 0.001;
const detail = { ...EVERYTHING, onsets: Float32Array.from([0, 0.0005]) };

describe("the stars seen from across the Galaxy", () => {
  it("grow fainter beyond 1 kiloparsec, in inverse proportion to the distance, down to a tenth", () => {
    expect(starsFade(0.1 * KPC)).toBe(1);
    expect(starsFade(KPC)).toBe(1);
    expect(starsFade(2 * KPC)).toBeCloseTo(0.5);
    expect(starsFade(5 * KPC)).toBeCloseTo(0.2);
    expect(starsFade(15 * KPC)).toBe(0.1);
    expect(starsFade(1000)).toBe(0.1);
  });

  it("fade the stars domain alone", () => {
    for (const distance of [0.1 * KPC, 4 * KPC, 30 * KPC]) {
      expect(domainOpacity("stars", detail, distance)).toBeCloseTo(detailOpacity(detail, distance) * starsFade(distance));
      expect(domainOpacity("galaxy_tracers", detail, distance)).toBe(detailOpacity(detail, distance));
      expect(domainOpacity("galaxy_model", detail, distance)).toBe(detailOpacity(detail, distance));
    }
  });
});
