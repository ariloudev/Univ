import { describe, expect, it } from "vitest";

import { FADE_MS, Fades } from "./fades.ts";

describe("a tile's fade", () => {
  it("rises from none to full over 0.3 seconds once the tile arrives", () => {
    expect(FADE_MS).toBe(300);
    const fades = new Fades();
    fades.arrive("t", 1000);
    expect(fades.opacity("t", 1000)).toBe(0);
    expect(fades.opacity("t", 1150)).toBeCloseTo(0.5);
    fades.drawn(1150);
    expect(fades.moving()).toBe(true);
    // The last frame of the fade, at 295 ms, leaves the tile short of full: a frame is still due.
    fades.drawn(1295);
    expect(fades.moving()).toBe(true);
    expect(fades.opacity("t", 1300)).toBe(1);
    fades.drawn(1310);
    expect(fades.moving()).toBe(false);
  });

  it("falls to none over 0.3 seconds once the tile goes, and only then is the tile gone", () => {
    const fades = new Fades();
    fades.arrive("t", 0);
    fades.leave("t", 1000);
    expect(fades.opacity("t", 1000)).toBe(1);
    expect(fades.opacity("t", 1150)).toBeCloseTo(0.5);
    expect(fades.gone(1150)).toEqual([]);
    fades.drawn(1150);
    expect(fades.moving()).toBe(true);
    // Drawn once the fade has ended but before the tile is let go: a frame is still due.
    fades.drawn(1300);
    expect(fades.moving()).toBe(true);
    expect(fades.gone(1300)).toEqual(["t"]);
    expect(fades.opacity("t", 1300)).toBe(0);
    fades.drawn(1300);
    expect(fades.moving()).toBe(false);
  });

  it("turns from where it is when the tile comes back before it is gone, or goes before it is in", () => {
    const fades = new Fades();
    fades.arrive("t", 0);
    fades.leave("t", 1000);
    fades.arrive("t", 1100);
    expect(fades.opacity("t", 1100)).toBeCloseTo(2 / 3);
    expect(fades.opacity("t", 1200)).toBe(1);
    expect(fades.gone(5000)).toEqual([]);
    fades.arrive("u", 0);
    fades.leave("u", 60);
    expect(fades.opacity("u", 60)).toBeCloseTo(0.2);
    expect(fades.gone(120)).toEqual(["u"]);
  });

  it("is none for a tile never drawn, which cannot go", () => {
    const fades = new Fades();
    fades.leave("t", 0);
    expect(fades.opacity("t", 0)).toBe(0);
    expect(fades.moving()).toBe(false);
  });
});
