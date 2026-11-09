import { describe, expect, it } from "vitest";

import { labelRows, linear, niceDomain } from "./chartGeometry.ts";

describe("the charts' geometry", () => {
  it("rounds a domain out to round ticks about the values", () => {
    expect(niceDomain([233.5, 348], 4)).toEqual({ domain: [200, 350], ticks: [200, 250, 300, 350] });
    expect(niceDomain([0, 124], 3)).toEqual({ domain: [0, 150], ticks: [0, 50, 100, 150] });
    expect(niceDomain([747, 783.4, 770], 3).ticks).toEqual([740, 760, 780, 800]);
  });

  it("gives a single value, or values all alike, a domain about them", () => {
    const { domain } = niceDomain([10, 10]);
    expect(domain[0]).toBeLessThan(10);
    expect(domain[1]).toBeGreaterThan(10);
    expect(niceDomain([])).toEqual({ domain: [0, 1], ticks: [0, 1] });
  });

  it("maps a domain onto a range linearly", () => {
    const x = linear([200, 400], [0, 100]);
    expect(x(200)).toBe(0);
    expect(x(300)).toBe(50);
    expect(x(400)).toBe(100);
  });

  it("sets a label that would meet another on a second row", () => {
    // The Sloan Great Wall's median and redshift lie a few pixels apart; Vela's walls far apart.
    expect(labelRows([200, 207])).toEqual([0, 1]);
    expect(labelRows([90, 230])).toEqual([0, 0]);
    expect(labelRows([207, 200, 300])).toEqual([1, 0, 0]);
  });
});
