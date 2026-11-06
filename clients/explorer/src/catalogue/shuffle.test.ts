import { describe, expect, it } from "vitest";

import { shuffled } from "./shuffle.ts";

const range = (n: number) => Array.from({ length: n }, (_, i) => i);

describe("a tile's shuffled order", () => {
  it("holds every point once, in the same order for the same id and another for another id", () => {
    const order = shuffled(range(1000), "cosmic_web/r07");
    expect([...order].sort((a, b) => a - b)).toEqual(range(1000));
    expect(order).not.toEqual(range(1000));
    expect(shuffled(range(1000), "cosmic_web/r07")).toEqual(order);
    expect(shuffled(range(1000), "cosmic_web/r06")).not.toEqual(order);
  });

  it("makes its first points, however many, an even sample of all", () => {
    // Points sorted by a value from 0 to 9,999, as a tile's are by id: the first tenth and the
    // first half of the shuffled order each have a mean near the whole's, 4,999.5.
    const order = shuffled(range(10_000), "stars/r");
    const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
    expect(Math.abs(mean(order.slice(0, 1000)) - 4999.5)).toBeLessThan(300);
    expect(Math.abs(mean(order.slice(0, 5000)) - 4999.5)).toBeLessThan(150);
    // Each tenth of the range gets close to its tenth of the first thousand.
    const tenths = range(10).map((d) => order.slice(0, 1000).filter((x) => Math.floor(x / 1000) === d).length);
    for (const n of tenths) expect(n).toBeGreaterThan(60);
  });

  it("leaves no point and one point as they are", () => {
    expect(shuffled([], "a")).toEqual([]);
    expect(shuffled([7], "a")).toEqual([7]);
  });
});
