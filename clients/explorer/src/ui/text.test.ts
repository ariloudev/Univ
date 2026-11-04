import { describe, expect, it } from "vitest";

import { pointText, preferredText, valueText } from "./text.ts";

describe("the words for an object's values", () => {
  it("give a published value with its unit and uncertainty as the source gave them", () => {
    expect(valueText({ value: 24.366, unit: "magnitude", uncertainty: { form: "symmetric", sigma: 0.045 } })).toBe("24.37 magnitude ± 0.045");
    expect(valueText({ value: 783, unit: "kiloparsec", uncertainty: { form: "asymmetric", minus: 25, plus: 25 } })).toBe("783 kiloparsec −25 +25");
    expect(valueText({ value: 0.77, unit: "megaparsec", uncertainty: { form: "none_reported" } })).toBe("0.77 megaparsec, no uncertainty reported");
    expect(valueText({ value: 2, unit: "parsec" })).toBe("2 parsec");
  });

  it("give the preferred distance with its fractional uncertainty, also as a distance", () => {
    const preferred = { rule: "r", measurement: { entry: 0, quantity: "distance", n: 0 }, distance_mpc: 0.7468, uncertainty: { form: "reported", total: 0.0207 } };
    expect(preferredText(preferred)).toBe("747 kpc (2.44 million ly) ± 2.1 % (± 15.5 kpc (50.4 thousand ly))");
    expect(preferredText({ ...preferred, uncertainty: { form: "none_reported", total: null } })).toBe("747 kpc (2.44 million ly), no uncertainty reported");
  });

  it("label a point with its id, scale domain, distance, and the source and kind of its distance", () => {
    expect(pointText({ id: 7, domain: "nearby_galaxies", distanceMpc: 0.7468, source: "Cosmicflows-4", kind: "measured" })).toBe(
      "7 · nearby galaxies · 747 kpc (2.44 million ly) · Cosmicflows-4 · measured",
    );
  });
});
