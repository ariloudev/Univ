import { describe, expect, it } from "vitest";

import { manifestOf } from "../test/fixtures.ts";
import { HIP_32754, HIP_32754_SOURCES } from "../test/records.ts";
import { decText, flagsOf, galacticOf, galacticText, masText, namesAndDesignations, raText, relationText } from "./recordText.ts";

describe("the record in words", () => {
  it("writes right ascension and declination in sexagesimal, carrying a rounded 60", () => {
    expect(raText(0)).toBe("00h 00m 00.00s");
    expect(raText(359.9999999)).toBe("00h 00m 00.00s");
    expect(raText(14.99999999 * 15)).toBe("15h 00m 00.00s");
    expect(decText(-5.3896667)).toBe("−05° 23′ 22.8″");
    expect(decText(89.99999)).toBe("+90° 00′ 00.0″");
    expect(decText(0)).toBe("+00° 00′ 00.0″");
  });

  it("turns an ICRS position into galactic longitude and latitude by the release's rotation", () => {
    const toGalactic = manifestOf("stars").rotations.to_galactic;
    // The north galactic pole (ESA 1997: α 192.85948°, δ +27.12825°) and the Galactic centre's
    // direction (l = 0, b = 0: α 266.40500°, δ −28.93617°).
    expect(galacticOf(192.85948, 27.12825, toGalactic).b).toBeCloseTo(90, 4);
    const centre = galacticOf(266.405, -28.93617, toGalactic);
    expect(Math.min(centre.l, 360 - centre.l)).toBeLessThan(1e-3);
    expect(centre.b).toBeCloseTo(0, 3);
  });

  it("merges designations by their text, keeping each source", () => {
    expect(namesAndDesignations(HIP_32754).designations.map((d) => [d.text, d.sources.length])).toEqual([
      ["HIP 32754", 2],
      ["Gaia DR3 1000166358456525568", 2],
      ["Gaia EDR3 1000166358456525568", 1],
    ]);
  });

  it("gives each flag its meaning where its source gives one", () => {
    const sources = new Map(HIP_32754_SOURCES.map((d) => [d.id, d]));
    const entry = HIP_32754.entries[4];
    if (entry === undefined) throw new Error("no entry");
    expect(flagsOf(entry, sources.get(entry.source))).toEqual([
      { column: "Sn", value: "5", meaning: "5-parameter solution; single star (d = 0, s = 5).", cite: "ReadMe of I/311, Note (1)" },
    ]);
    expect(flagsOf({ ...entry, flags: { n: 3 } }, undefined)).toEqual([{ column: "n", value: "3", meaning: undefined, cite: undefined }]);
  });

  it("words how the object stands to a group", () => {
    expect(relationText(["dominant_member_of", "member_of"])).toBe("The dominant galaxy of the group");
    expect(relationText(["member_of"])).toBe("A member of the group");
    expect(relationText(["dominant_member", "member"])).toBe("The group's dominant galaxy");
    expect(relationText(["member"])).toBe("A member of this group");
  });

  it("writes a position's error without exponents, and a longitude that rounds to 360 as 0", () => {
    expect(masText(1500)).toBe("± 1,500 mas");
    expect(masText(0.0101)).toBe("± 0.01 mas");
    expect(masText(null)).toBe("no uncertainty reported");
    expect(galacticText({ l: 359.99996, b: -0.5 })).toBe("l 0.0000°, b −0.5000°");
  });

  it("merges only designations in a catalogue's form across sources, not two sources' bare numbers", () => {
    const record = {
      ...HIP_32754,
      designations: [
        { source: "cosmicflows-4-galaxies", identifier: "2557" },
        { source: "some-catalogue", identifier: "2557" },
      ],
    };
    expect(namesAndDesignations(record).designations.map((d) => d.sources)).toEqual([["cosmicflows-4-galaxies"], ["some-catalogue"]]);
  });

  it("gives a designation that reads as a proper name once, as the name, where it is in its catalogue's form or a source of the name gives it", () => {
    const record = {
      ...HIP_32754,
      names: [
        { name: "Sirius", source: "iau-star-names" },
        { name: "M 31", source: "openngc" },
      ],
      designations: [
        { source: "iau-star-names", identifier: "Sirius" },
        { source: "openngc", identifier: "M 31", designation: "M 31" },
        { source: "some-catalogue", identifier: "m31" },
        { source: "hipparcos-2007", identifier: "32349", designation: "HIP 32349" },
      ],
    };
    expect(namesAndDesignations(record)).toEqual({
      names: [
        { text: "Sirius", sources: ["iau-star-names"] },
        { text: "M 31", sources: ["openngc"] },
      ],
      // Another catalogue's bare "m31" is its own number, not a name it gives.
      designations: [
        { text: "m31", sources: ["some-catalogue"] },
        { text: "HIP 32349", sources: ["hipparcos-2007"] },
      ],
    });
  });
});
