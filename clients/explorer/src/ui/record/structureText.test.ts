import { describe, expect, it } from "vitest";

import { readPanelRecord, readStructure } from "../../api/structures.ts";
import vela from "../../test/records/st-vela-supercluster.json";

import { R12_GREAT_ATTRACTOR, R12_HUGE_LQG, R12_KBC_VOID, R12_LANIAKEA, R12_LOCAL_VOID, R12_SLOAN_GREAT_WALL, R12_VELA } from "../../test/records/index.ts";
import { depthBand, disputedBy, measureLines, memberStrip, roleOf, sizeView, skyView, structureLede, structureParts, structureSubtitle } from "./structureText.ts";

const PLANCK_2018 = { name: "Planck 2018 (Planck Collaboration 2020)", hubble_constant: 67.66, omega_matter: 0.3111 };

const lede = (s: typeof R12_VELA) => structureLede(s, structureParts(s, PLANCK_2018));

describe("the structure panel's sentence", () => {
  it("says what kind of structure it is, how far away, and how long its longest length", () => {
    expect(lede(R12_VELA)).toBe("A supercluster 234–348 Mpc away, spanning at least 124 Mpc.");
    expect(lede(R12_KBC_VOID)).toBe("A void about us, out to 310 Mpc.");
    expect(lede(R12_HUGE_LQG)).toBe("A large quasar group of 73 members 3.81–4.23 Gpc away, spanning 1.28 Gpc.");
    expect(lede(R12_GREAT_ATTRACTOR)).toBe("An attractor about 64.1 Mpc away.");
    expect(lede(R12_LANIAKEA)).toBe("A basin of attraction about 72.2 Mpc away, spanning 178 Mpc.");
    expect(lede(R12_SLOAN_GREAT_WALL)).toBe("A wall 226–388 Mpc away, spanning 471 Mpc.");
  });

  it("names its other names, kind and domain beneath its name, and its status where it is not plainly observed", () => {
    expect(structureSubtitle(R12_HUGE_LQG)).toBe("U1.27 · Huge Large Quasar Group · quasar group · cosmic web domain · statistical claim");
    expect(structureSubtitle(R12_VELA)).toBe("VSCL · supercluster · cosmic web domain");
  });
});

describe("the role of each value of a structure's record", () => {
  const value = (unit: string) => ({ kind: "measured", quantity: { value: 1, unit } });

  it("takes lengths, depths and other measures apart, an infall velocity being no depth", () => {
    expect(roleOf("wall_1_velocity", value("kilometre_per_second"))).toBe("depth");
    expect(roleOf("infall_velocity_at_local_group", value("kilometre_per_second"))).toBe("measure");
    expect(roleOf("diameter_in_velocity", value("kilometre_per_second"))).toBe("length");
    expect(roleOf("Radius_if_empty", value("megaparsec"))).toBe("length");
    expect(roleOf("radius_to_centre_at_our_position", value("megaparsec"))).toBe("measure");
    expect(roleOf("median_distance", value("megaparsec"))).toBe("depth");
    expect(roleOf("edge_redshift", value("dimensionless"))).toBe("depth");
    expect(roleOf("luminosity_density_ratio_beyond", value("dimensionless"))).toBe("measure");
  });

  it("leaves a radius out of the span its sentence gives, whatever its case", () => {
    const voidOnly = { ...R12_KBC_VOID, record: { ...R12_KBC_VOID.record, values: { radius_if_empty: { kind: "inferred", quantity: { value: 16, unit: "megaparsec" } } } } };
    expect(lede(voidOnly)).toBe("A void about us, out to 310 Mpc.");
  });
});

describe("the structure panel's depth", () => {
  it("places a sky region's velocities at the comoving distance of z = cz/c, with the velocities within marked", () => {
    const band = depthBand(R12_VELA, PLANCK_2018);
    // cz 16,000 and 24,000 km/s; the walls W1 and W2 at 18,000 and 22,000 km/s.
    expect(band?.from).toBeCloseTo(233.5, 1);
    expect(band?.to).toBeCloseTo(348.0, 1);
    expect(band?.marks.map((m) => [m.label, Number(m.value.toFixed(1))])).toEqual([
      ["wall 1", 262.3],
      ["wall 2", 319.5],
    ]);
    expect(band?.used).toEqual(["wall_1_velocity", "wall_2_velocity"]);
    expect(band?.note).toBe(
      "Published as cz 16,000 to 24,000 km/s; placed by comoving distance of the redshift z = cz / c, taken as published and not moved, under the release cosmology, for each bound.",
    );
    // The bounds are where Univ Server placed them; without the release cosmology, the walls are not.
    expect(depthBand(R12_VELA, undefined)?.marks).toEqual([]);
  });

  it("rescales a published distance or radius to the release cosmology, and marks a redshift within it", () => {
    const wall = depthBand(R12_SLOAN_GREAT_WALL, PLANCK_2018);
    // 215 and 370 Mpc at H₀ = 71: 215 × 71 / 67.66 = 225.6, 370 × 71 / 67.66 = 388.3.
    expect([wall?.from.toFixed(1), wall?.to.toFixed(1)]).toEqual(["225.6", "388.3"]);
    expect(wall?.marks.map((m) => m.label)).toEqual(["median", "z"]);
    expect(wall?.note).toBe(
      "Published as 215–370 Mpc; placed by the length as published, rescaled from H0 = 71 km/s/Mpc to the release's by the ratio of Hubble constants, for each bound.",
    );
    const kbc = depthBand(R12_KBC_VOID, PLANCK_2018);
    expect([kbc?.from, kbc?.to.toFixed(1)]).toEqual([0, "310.4"]);
    expect(kbc?.marks.map((m) => m.label)).toEqual(["edge z"]);
    expect(depthBand(R12_GREAT_ATTRACTOR, PLANCK_2018)).toBeUndefined();
  });

  it("gives a structure placed by its members each member's distance, with its mean redshift marked", () => {
    const members = memberStrip(R12_HUGE_LQG, PLANCK_2018);
    expect(members?.values).toHaveLength(73);
    expect(members?.marks.map((m) => m.label)).toEqual(["mean z"]);
    expect(members?.note).toBe("73 members, each at the comoving distance of its redshift.");
    expect(memberStrip(R12_VELA, PLANCK_2018)).toBeUndefined();
  });
});

describe("the structure panel's sky, size and measures", () => {
  it("gives a sky region's ranges in its frame, right ascension in hours, and a published position", () => {
    expect(skyView(R12_VELA)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["l 255° to 285°", "b −10° to +10°"]);
    expect(skyView(R12_SLOAN_GREAT_WALL)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["RA 8h 42m to 13h 42m", "Dec −2° to +2°"]);
    expect(skyView(R12_GREAT_ATTRACTOR)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["l, b 307°, +9°", "Velocity 4,350 ± 350 km/s"]);
    expect(skyView(R12_LANIAKEA)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["SGX, SGY, SGZ −4,700, 1,300, −500 km/s"]);
    expect(skyView(R12_KBC_VOID)).toBeUndefined();
  });

  it("gives its lengths in the release cosmology, one group for each unit, a lower limit open, with what they were published as", () => {
    const [mpc] = sizeView(R12_VELA, PLANCK_2018);
    expect(mpc?.rows.map((r) => [r.label, r.text, r.open])).toEqual([
      ["Long", "≥ 119", true],
      ["Short", "≥ 93.1", true],
      ["Wall 1 extent", "≥ 124", true],
    ]);
    expect(mpc?.note).toBe("Published as 115, 90, 120 Mpc at H₀ = 70, drawn at H₀ = 67.66.");
    expect(sizeView(R12_LOCAL_VOID, PLANCK_2018).map((g) => [g.unit, g.rows.length])).toEqual([
      ["Mpc", 2],
      ["km/s", 4],
    ]);
  });

  it("gives its other values on lines, each with its paper only where it cites more than one", () => {
    expect(measureLines(R12_VELA, ["wall_1_velocity", "wall_2_velocity"]).map((l) => `${l.label}: ${l.value} (${l.source})`)).toEqual([
      "Extent latitude: ≥ 20° ()",
      "Extent longitude: ≥ 25° ()",
    ]);
    expect(measureLines(R12_GREAT_ATTRACTOR, []).find((l) => l.label === "Infall velocity at local group")).toEqual({
      label: "Infall velocity at local group",
      value: "570 ± 60 km/s",
      source: "Lynden-Bell et al. 1988",
      kind: "reconstructed",
    });
  });

  it("names each paper that disputes it", () => {
    expect(disputedBy(R12_KBC_VOID)).toEqual(["Kenworthy et al. 2019"]);
    expect(disputedBy(R12_HUGE_LQG)).toEqual(["Nadathur 2013"]);
    expect(disputedBy(R12_VELA)).toEqual([]);
  });
});

describe("a structure's record of a shape the panel does not know", () => {
  const raw = (record: Record<string, unknown>) => readStructure({ ...vela, record: { ...vela.record, ...record } });

  it("gives a sky region given by its centre and size as those", () => {
    const [lon, lat] = vela.record.geometry.SkyRegion.lon;
    const region = { frame: "galactic", centre: [lon, lat], size: [lat, lat] };
    const detail = raw({ geometry: { SkyRegion: region } });
    expect(skyView(detail)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["l, b 255°, +285°", "Across 285° × 285°"]);
  });

  it("is still drawn where its record takes a shape the panel does not know, and the panel says what it could not read", () => {
    const detail = readStructure(vela);
    const odd = { ...detail, record: { ...detail.record, disputes: [{}] } };
    expect(readPanelRecord(odd)).toEqual({ problem: "record.disputes.0.cite: Invalid input: expected object, received undefined" });
    expect(skyView(raw({ geometry: null, position: { form: "spherical", frame: "galactic", lon: null, lat: null } }))?.lines).toEqual([]);
  });

});
