// @vitest-environment node
import { describe, expect, it } from "vitest";

import { hiddenInAddress } from "../ui/hidden.ts";
import { MASK_BITS, NO_MASKS, isPointShown, masksOf, shownIn } from "./masks.ts";

const DOMAINS = [
  { name: "stars", manifest: { sources: [{ index: 0, id: "gaia-dr3-bright" }, { index: 3, id: "hipparcos-2007" }] } },
  { name: "cosmic_web", manifest: { sources: [{ index: 0, id: "twomrs" }, { index: 1, id: "sdss-dr17-galaxies" }] } },
  { name: "nearby_galaxies", manifest: { sources: [{ index: 5, id: "twomrs" }] } },
];

describe("the masks of what is turned off", () => {
  it("set a source's bit at its index in each domain's manifest", () => {
    const masks = masksOf(hiddenInAddress("?hide=source:hipparcos-2007,source:sdss-dr17-galaxies"), DOMAINS);
    expect(masks.sources.get("stars")).toBe(1 << 3);
    expect(masks.sources.get("cosmic_web")).toBe(1 << 1);
    expect(masks.sources.get("nearby_galaxies")).toBe(0);
  });

  it("turn off a source in every domain that has it, at its own index in each", () => {
    const masks = masksOf(hiddenInAddress("?hide=source:twomrs"), DOMAINS);
    expect(masks.sources.get("cosmic_web")).toBe(1);
    expect(masks.sources.get("nearby_galaxies")).toBe(1 << 5);
  });

  it("set a kind's bit at its index among the kinds", () => {
    expect(masksOf(hiddenInAddress("?hide=kind:measured,kind:reconstructed"), DOMAINS).kinds).toBe(0b1001);
  });

  it("cannot turn off a source beyond the bits a mask holds: its points stay drawn", () => {
    const wide = [{ name: "wide", manifest: { sources: [{ index: MASK_BITS, id: "too-many" }] } }];
    const masks = masksOf(hiddenInAddress("?hide=source:too-many"), wide);
    expect(masks.sources.get("wide")).toBe(0);
    expect(isPointShown(masks, "wide", MASK_BITS, 0)).toBe(true);
  });

  it("tell which points are drawn: by domain, source and kind", () => {
    const masks = masksOf(hiddenInAddress("?hide=domain:nearby_galaxies,source:hipparcos-2007,kind:estimated"), DOMAINS);
    expect(isPointShown(masks, "nearby_galaxies", 5, 0)).toBe(false);
    expect(isPointShown(masks, "stars", 3, 0)).toBe(false);
    expect(isPointShown(masks, "stars", 0, 1)).toBe(false);
    expect(isPointShown(masks, "stars", 0, 0)).toBe(true);
    expect(shownIn(masks, "nearby_galaxies", [5], [0])).toBe("none");
    expect(shownIn(NO_MASKS, "stars", [3], [0])).toBe("all");
    const shown = shownIn(masks, "stars", [3, 0], [0, 0]);
    expect(typeof shown === "function" ? [shown(0), shown(1)] : shown).toEqual([false, true]);
  });
});
