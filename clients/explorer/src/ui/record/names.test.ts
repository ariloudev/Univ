import { describe, expect, it } from "vitest";

import { paperShort, sourceShort } from "./names.ts";

describe("the record's short names", () => {
  it("name a source by its short name, else by its title, else by its id", () => {
    expect(sourceShort("gaia-dr3-bright", "Gaia DR3 bright stars (G < 10)")).toBe("Gaia DR3");
    expect(sourceShort("updated-nearby-galaxies")).toBe("Karachentsev 2013");
    expect(sourceShort("a-new-survey", "A New Survey")).toBe("A New Survey");
    expect(sourceShort("a-new-survey")).toBe("a-new-survey");
  });

  it("name a paper by its first author and year, and keep the rest of its reference", () => {
    expect(paperShort("Kraan-Korteweg, Cluver, Bilicki, Jarrett, Colless, Elagali, Böhringer & Chon 2017, MNRAS Letters 466, L29")).toEqual({
      short: "Kraan-Korteweg et al. 2017",
      rest: "MNRAS Letters 466, L29",
    });
    expect(paperShort("Keenan, Barger & Cowie 2013, ApJ 775, 62")).toEqual({ short: "Keenan et al. 2013", rest: "ApJ 775, 62" });
    expect(paperShort("Nadathur 2013, MNRAS 434, 398")).toEqual({ short: "Nadathur 2013", rest: "MNRAS 434, 398" });
    expect(paperShort("A reference with no year")).toEqual({ short: "A reference with no year", rest: "" });
  });
});
