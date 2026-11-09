import { describe, expect, it } from "vitest";

import { R12_DRACO, R12_ERASS, R12_GROUP, R12_HIP_32754, R12_M31, R12_NGC, R12_SDSS } from "../../test/records/index.ts";
import { figures } from "../../scene/format.ts";
import { sourceShort } from "./names.ts";
import { brightnessLines, cataloguesText, distanceView, drawnColourOf, hasProperMotion, motionLines, objectLede, velocityView } from "./objectRecordText.ts";

const name = (id: string) => sourceShort(id);

describe("the object panel's sentence", () => {
  it("says what the object is as far as its record tells, how far, and in which domain", () => {
    expect(objectLede(R12_HIP_32754)).toBe("A K-type star 34.7 pc away, in the stars domain.");
    expect(objectLede(R12_M31)).toBe("A galaxy 747 kpc away, the brightest of its group, in the nearby galaxies domain.");
    // The fixture keeps twelve of the group's members.
    expect(objectLede(R12_GROUP)).toBe("A group of 12 galaxies 16.2 Mpc away, in the cosmic web domain.");
    expect(objectLede(R12_ERASS)).toBe("A galaxy cluster or group 1.58 Gpc away, by its photometric redshift, in the cosmic web domain.");
    // SDSS DR17's class is not kept in the record, so the object is named no more than that.
    expect(objectLede(R12_SDSS)).toMatch(/^An object 4 Gpc away, by its spectroscopic redshift/);
    expect(objectLede({ ...R12_SDSS, preferred: null, position: null })).toBe("An object, with a sky position only, so not placed.");
  });
});

describe("the object panel's distance", () => {
  it("leads with the preferred distance and its uncertainty, in the unit that suits it", () => {
    const view = distanceView(R12_M31, name);
    expect(view).toMatchObject({ figure: "746.8", unit: "kpc", minus: "15", plus: "16", aside: "2.44 million ly", kind: "measured" });
    expect(view?.rule).toMatch(/^Preferred: Cosmicflows-4, by the rule “measured redshifts first/);
    expect(distanceView(R12_ERASS, name)).toMatchObject({ figure: "1.584", unit: "Gpc", aside: "from z = 0.3948 ± 0.014", kind: "estimated" });
    // A redshift gives no range in distance: the lead takes the uncertainty Univ Server gives the
    // preferred distance, a fraction of it.
    const total = R12_ERASS.preferred?.uncertainty.total ?? 0;
    expect(distanceView(R12_ERASS, name)?.plus).toBe(figures(total * 1.584, 2));
  });

  it("puts every distance and parallax on one axis, a parallax at its inverse and labelled so, the preferred marked", () => {
    const rows = distanceView(R12_HIP_32754, name)?.rows ?? [];
    expect(rows.map((r) => r.label)).toEqual(["Bailer-Jones 2021, geometric", "Bailer-Jones 2021, photogeometric", "Gaia DR3, 1/ϖ", "GCNS", "GCNS, 1/ϖ", "Hipparcos 2007, 1/ϖ"]);
    expect(rows.filter((r) => r.preferred).map((r) => r.label)).toEqual(["Bailer-Jones 2021, geometric"]);
    // Hipparcos 2007: ϖ 28.8 ± 1.83 mas, so 1/ϖ 34.72 pc, from 1/30.63 to 1/26.97 mas.
    const hipparcos = rows.at(-1);
    expect(hipparcos?.value).toBeCloseTo(34.722, 3);
    expect(hipparcos?.low).toBeCloseTo(32.648, 3);
    expect(hipparcos?.high).toBeCloseTo(37.078, 3);
    expect(hipparcos?.text).toBe("34.72 pc, the inverse of its parallax, measured");
    expect(distanceView(R12_HIP_32754, name)?.parallaxes.map((p) => p.text)).toEqual(["Gaia DR3 28.8 ± 0.014 mas", "GCNS 28.8 ± 0.014 mas", "Hipparcos 2007 28.8 ± 1.8 mas"]);
  });

  it("draws no plot for a single distance", () => {
    expect(distanceView(R12_NGC, name)?.rows).toEqual([]);
    expect(distanceView({ ...R12_NGC, preferred: null }, name)).toBeUndefined();
  });
});

describe("the object panel's brightness, colour and motion", () => {
  it("gives one line for each band, each colour and each type, with its source by its short name", () => {
    expect(brightnessLines(R12_HIP_32754, name).map((l) => `${l.label} ${l.value} ${l.source} ${l.kind}`)).toEqual([
      "G 9.879 Gaia DR3 measured",
      "Hp 10.5 Hipparcos 2007 measured",
      "BP−RP 1.529 Gaia DR3 measured",
      "B−V 1.277 ± 0.02 Hipparcos 2007 measured",
      "V−I 1.46 Hipparcos 2007 measured",
      "Type K Gaia DR3 estimated",
    ]);
    expect(brightnessLines(R12_NGC, name).map((l) => l.label)).toEqual(["Ks", "Kc"]);
    // The index the view draws the star by: Gaia's published BP−RP before Hipparcos's B−V.
    expect(drawnColourOf(R12_HIP_32754)).toEqual({ index: expect.closeTo(1.5287, 4), bands: "BP-RP", source: "gaia-dr3-bright" });
    expect(drawnColourOf(R12_NGC)).toBeUndefined();
  });

  it("gives a star's proper motion as its total and position angle, from one source's two components", () => {
    expect(hasProperMotion(R12_HIP_32754)).toBe(true);
    expect(hasProperMotion(R12_M31)).toBe(false);
    // Gaia DR3: μα* −15.50, μδ −221.08 mas/yr: 221.6 mas/yr, PA 180° + atan(15.50/221.08) = 184°.
    expect(motionLines(R12_HIP_32754, name)).toEqual([
      { label: "Proper", value: "221.6 mas/yr to PA 184°", source: "Gaia DR3", kind: "measured" },
      { label: "Radial", value: "27.25 ± 0.2 km/s", source: "Gaia DR3", kind: "measured" },
    ]);
  });

  it("gives a small redshift published as z as cz, its uncertainty converted with it", () => {
    const desi = R12_DRACO.measurements.find((m) => m.source === "desi-dr1" && m.quantity === "redshift");
    if (desi === undefined) throw new Error("no DESI redshift");
    const one = { ...desi, native: { value: -0.00124, unit: "dimensionless", uncertainty: { form: "symmetric", sigma: 4.9e-6 } } };
    const line = velocityView({ ...R12_NGC, measurements: [one] }, name).lines[0];
    // cz = −0.00124 c = −371.7 km/s, and σ = 4.9e-6 c = 1.5 km/s.
    expect(line?.value).toBe("−371.7 ± 1.5 km/s");
  });

  it("gives a galaxy's velocity by frame, once for each source, and many redshifts of one source as a strip", () => {
    expect(velocityView(R12_NGC, name).lines.map((l) => `${l.label}: ${l.value} (${l.source})`)).toEqual([
      "CMB frame: 1,317 km/s (Cosmicflows-4)",
      "Heliocentric: 997 ± 5 km/s (2M++)",
      "Photometric z: z = 0.000476 ± 0.015 (2MPZ)",
    ]);
    expect(velocityView(R12_NGC, name).strip).toBeUndefined();
    const { strip } = velocityView(R12_M31, name);
    expect(strip?.source).toBe("DESI DR1");
    expect(strip?.values).toHaveLength(12);
    expect(strip?.mark).toBe(-296);
    expect(velocityView(R12_DRACO, name).lines.map((l) => l.source)).toEqual(["Karachentsev 2013"]);
  });
});

describe("the object panel's catalogues", () => {
  it("says how many entries of how many catalogues hold the object, by their short names", () => {
    expect(cataloguesText(R12_M31, name)).toEqual({
      title: "In 20 entries of 9 catalogues",
      detail: "DESI DR1 12, Karachentsev 2013, OpenNGC, 2MPZ, WISE × SuperCOSMOS, 2M++, 2MRS, Cosmicflows-4, McConnachie 2012.",
    });
    expect(cataloguesText(R12_ERASS, name)).toEqual({ title: "In 1 catalogue", detail: "eRASS1." });
  });
});
