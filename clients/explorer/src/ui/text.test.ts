import { describe, expect, it } from "vitest";

import { pointText, preferredText, valueText } from "./text.ts";

describe("the words for an object's values", () => {
  it("give a published value with its unit and uncertainty as the source gave them", () => {
    expect(valueText({ value: 24.366, unit: "magnitude", uncertainty: { form: "symmetric", sigma: 0.045 } })).toBe("24.37 ± 0.045 magnitude");
    expect(valueText({ value: 783, unit: "kiloparsec", uncertainty: { form: "asymmetric", minus: 25, plus: 25 } })).toBe("783 −25 +25 kiloparsec");
    expect(valueText({ value: 0.77, unit: "megaparsec", uncertainty: { form: "none_reported" } })).toBe("0.77 megaparsec, no uncertainty reported");
    expect(valueText({ value: 2, unit: "parsec" })).toBe("2 parsec");
  });

  it("give the preferred distance with its fractional uncertainty, also as a distance", () => {
    const preferred = { rule: "r", measurement: { entry: 0, quantity: "distance", n: 0 }, distance_mpc: 0.7468, uncertainty: { form: "reported", total: 0.0207 } };
    expect(preferredText(preferred)).toBe("747 kpc (2.44 million ly) ± 2.1 % (± 15.5 kpc (50.4 thousand ly))");
    expect(preferredText({ ...preferred, uncertainty: { form: "none_reported", total: null } })).toBe("747 kpc (2.44 million ly), no uncertainty reported");
  });

  it("label a point with its id, scale domain, distance, and the source and kind of its distance", () => {
    expect(pointText({ id: 7, domain: "nearby_galaxies", place: [0.7468, 0, 0], distanceMpc: 0.7468, source: "Cosmicflows-4", kind: "measured" })).toBe(
      "7 · nearby galaxies · 747 kpc (2.44 million ly) · Cosmicflows-4 · measured",
    );
  });
});

describe("an uncertainty", () => {
  it("is said in the form its source published it: a range is no error, a limit no value", () => {
    expect(valueText({ value: 0.2, unit: "dimensionless", uncertainty: { form: "interval", minus: 0.1, plus: 0.1 } })).toBe(
      "0.2 dimensionless, stated to lie within −0.1 +0.1 (a range, not an error)",
    );
    expect(valueText({ value: 5, unit: "megaparsec", uncertainty: { form: "upper_limit" } })).toBe("< 5 megaparsec (an upper limit)");
    expect(valueText({ value: 5, unit: "megaparsec", uncertainty: { form: "lower_limit" } })).toBe("> 5 megaparsec (a lower limit)");
    expect(valueText({ value: 120, unit: "parsec", uncertainty: { form: "credible_interval", minus: 2, plus: 3, probability: 0.6827 } })).toBe(
      "120 −2 +3 parsec (0.6827 credible)",
    );
  });

  it("keeps the significant figures of a small value", () => {
    expect(valueText({ value: 0.0001234, unit: "degree", uncertainty: { form: "symmetric", sigma: 0.00001234 } })).toBe(
      "0.0001234 ± 0.00001234 degree",
    );
  });
});
