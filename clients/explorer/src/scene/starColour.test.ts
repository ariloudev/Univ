import { describe, expect, it } from "vitest";

import { NO_COLOUR_GREY, blackBodyColour, starColour, temperatureOf } from "./starColour.ts";

describe("temperatureOf", () => {
  it("gives the Sun's colour index about 5,770 K and G2V", () => {
    const sun = temperatureOf(0.65, "B-V");
    expect(sun.teff).toBeGreaterThan(5740);
    expect(sun.teff).toBeLessThan(5800);
    expect(sun.type).toBe("G2V");
    expect(sun.clamped).toBeUndefined();
  });

  it("gives B−V 1.50 about 3,700 K, an early M dwarf", () => {
    const red = temperatureOf(1.5, "B-V");
    expect(red.teff).toBeGreaterThan(3500);
    expect(red.teff).toBeLessThan(3800);
    // Midway between M1.5V and M2V: an early M dwarf either way.
    expect(red.type).toMatch(/^M[0-2]/);
  });

  it("gives B−V −0.30 over 25,000 K", () => {
    expect(temperatureOf(-0.3, "B-V").teff).toBeGreaterThan(25_000);
  });

  it("reads BP−RP along its own column", () => {
    const sun = temperatureOf(0.823, "BP-RP");
    expect(sun.type).toBe("G2V");
    expect(sun.teff).toBeCloseTo(5770, -1);
  });

  it("takes an index beyond the sequence at its end, and says which", () => {
    expect(temperatureOf(-1, "B-V")).toMatchObject({ teff: 44_900, type: "O3V", clamped: "hot" });
    expect(temperatureOf(9, "B-V").clamped).toBe("cool");
    expect(temperatureOf(-1, "BP-RP")).toMatchObject({ type: "B9V", clamped: "hot" });
  });

  it("rises in temperature as the index falls", () => {
    const indices = [-0.3, 0, 0.3, 0.65, 1, 1.5, 2];
    const teffs = indices.map((i) => temperatureOf(i, "B-V").teff);
    expect([...teffs].sort((a, b) => b - a)).toEqual(teffs);
  });
});

describe("blackBodyColour", () => {
  it("draws a hot star bluer than a cool one", () => {
    const [hr, , hb] = blackBodyColour(30_000);
    const [cr, , cb] = blackBodyColour(3000);
    expect(hb / hr).toBeGreaterThan(1);
    expect(cb / cr).toBeLessThan(0.5);
  });

  it("scales the brightest channel to full", () => {
    for (const teff of [2500, 5770, 10_000, 40_000]) expect(Math.max(...blackBodyColour(teff))).toBeCloseTo(1, 6);
  });

  it("gives the Sun a near-white, slightly warm colour", () => {
    const [r, g, b] = blackBodyColour(5770);
    expect(r).toBeCloseTo(1, 6);
    expect(g).toBeGreaterThan(0.85);
    expect(b).toBeGreaterThan(0.75);
    expect(b).toBeLessThan(r);
  });
});

describe("starColour", () => {
  it("draws a star without a colour index in neutral grey", () => {
    expect(starColour(undefined)).toEqual(NO_COLOUR_GREY);
    const [r, g, b] = NO_COLOUR_GREY;
    expect(r).toBe(g);
    expect(g).toBe(b);
  });

  it("draws a red star red and keeps the points' half opacity", () => {
    const [r, g, b, a] = starColour({ index: 1.85, bands: "B-V" });
    expect(r).toBeCloseTo(1, 6);
    expect(b).toBeLessThan(g);
    expect(a).toBe(0.5);
  });
});
