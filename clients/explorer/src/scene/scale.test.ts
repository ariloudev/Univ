import { describe, expect, it } from "vitest";

import { LONGEST, SHORTEST, scaleBar } from "./scale.ts";

describe("the scale bar", () => {
  it("is a round length of parsecs, kiloparsecs or megaparsecs between 60 and 160 pixels", () => {
    for (let exponent = -10; exponent <= 2; exponent += 0.137) {
      const mpcPerPixel = 10 ** exponent;
      const bar = scaleBar(mpcPerPixel);
      expect(bar.pixels).toBeGreaterThanOrEqual(SHORTEST - 1e-6);
      expect(bar.pixels).toBeLessThanOrEqual(LONGEST);
      const mantissa = bar.parsecs / 10 ** Math.floor(Math.log10(bar.parsecs) + 1e-9);
      expect([1, 2, 5].some((m) => Math.abs(mantissa - m) < 1e-6)).toBe(true);
      expect(bar.label).toMatch(/^[\d,.]+ (pc|kpc|Mpc)$/);
    }
  });

  it("is labelled in the unit that suits its length, and changes with the distance", () => {
    expect(scaleBar(1 / 1e6).label).toBe("100 pc");
    expect(scaleBar(10 / 1e6).label).toBe("1 kpc");
    expect(scaleBar(1).label).toBe("100 Mpc");
    expect(scaleBar(100).label).toBe("10,000 Mpc");
    expect(scaleBar(1e-10).label).toBe("0.01 pc");
  });
});
