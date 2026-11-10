// @vitest-environment node
import { describe, expect, it } from "vitest";

import { halfOf } from "./dust.ts";

/** The value of the half float `bits`. */
function valueOf(bits: number): number {
  const exponent = (bits >> 10) & 0x1f;
  const mantissa = bits & 0x3ff;
  const magnitude = exponent === 0 ? mantissa * 2 ** -24 : (1 + mantissa / 1024) * 2 ** (exponent - 15);
  return bits & 0x8000 ? -magnitude : magnitude;
}

describe("the dust texture's half floats", () => {
  it("hold the densities, in thousandths, to within a half float's precision", () => {
    expect(halfOf(0)).toBe(0);
    expect(halfOf(1)).toBe(0x3c00);
    expect(halfOf(0.5)).toBe(0x3800);
    expect(halfOf(1e9)).toBe(0x7bff);
    // The real map's densities times 1,000: its median, its 99th percentile and its densest cell.
    for (const value of [0.0109, 1.35, 39.5, 1e-5]) {
      expect(Math.abs(valueOf(halfOf(value)) - value)).toBeLessThanOrEqual(Math.max(value * 2 ** -10, 2 ** -24));
    }
  });
});
