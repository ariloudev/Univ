import { describe, expect, it } from "vitest";

import { readStructure } from "../../api/structures.ts";
import vela from "../../test/records/st-vela-supercluster.json";

import { R12_GREAT_ATTRACTOR, R12_HUGE_LQG, R12_KBC_VOID, R12_LANIAKEA, R12_LOCAL_VOID, R12_SLOAN_GREAT_WALL, R12_VELA } from "../../test/records/index.ts";
import { depthBand, disputedBy, measureLines, memberStrip, sizeView, skyView, structureLede, structureSubtitle } from "./structureText.ts";

const PLANCK_2018 = { name: "Planck 2018 (Planck Collaboration 2020)", hubble_constant: 67.66, omega_matter: 0.3111 };

describe("the structure panel's sentence", () => {
  it("says what kind of structure it is, how far away, and how long its longest length", () => {
    expect(structureLede(R12_VELA, PLANCK_2018)).toBe("A supercluster 234–348 Mpc away, spanning at least 124 Mpc.");
    expect(structureLede(R12_KBC_VOID, PLANCK_2018)).toBe("A void about us, out to 310 Mpc.");
    expect(structureLede(R12_HUGE_LQG, PLANCK_2018)).toBe("A large quasar group of 73 members 3.81–4.23 Gpc away, spanning 1.28 Gpc.");
    expect(structureLede(R12_GREAT_ATTRACTOR, PLANCK_2018)).toBe("An attractor about 64.1 Mpc away.");
    expect(structureLede(R12_LANIAKEA, PLANCK_2018)).toBe("A basin of attraction about 72.2 Mpc away, spanning 178 Mpc.");
    expect(structureLede(R12_SLOAN_GREAT_WALL, PLANCK_2018)).toBe("A wall 226–388 Mpc away, spanning 471 Mpc.");
  });

  it("names its other names, kind and domain beneath its name, and its status where it is not plainly observed", () => {
    expect(structureSubtitle(R12_HUGE_LQG)).toBe("U1.27 · Huge Large Quasar Group · quasar group · cosmic web domain · statistical claim");
    expect(structureSubtitle(R12_VELA)).toBe("VSCL · supercluster · cosmic web domain");
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
    expect(band?.note).toBe("Published as cz 16,000 to 24,000 km/s, drawn at the comoving distance of z = cz/c under Planck 2018.");
    expect(depthBand(R12_VELA, undefined)).toBeUndefined();
  });

  it("rescales a published distance or radius to the release cosmology, and marks a redshift within it", () => {
    const wall = depthBand(R12_SLOAN_GREAT_WALL, PLANCK_2018);
    // 215 and 370 Mpc at H₀ = 71: 215 × 71 / 67.66 = 225.6, 370 × 71 / 67.66 = 388.3.
    expect([wall?.from.toFixed(1), wall?.to.toFixed(1)]).toEqual(["225.6", "388.3"]);
    expect(wall?.marks.map((m) => m.label)).toEqual(["median", "z"]);
    expect(wall?.note).toBe("Published as 215–370 Mpc at H₀ = 71, drawn at Planck 2018's H₀ = 67.66.");
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
    expect(skyView(R12_GREAT_ATTRACTOR)?.lines.map((l) => `${l.label} ${l.value}`)).toEqual(["l, b 307°, +9°", "Velocity 4,350 km/s"]);
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
      value: "570 km/s",
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

  it("is still read, and drawn, where a part only the panel reads takes another shape", () => {
    const detail = raw({ position: { form: "spherical", frame: "galactic", lon: null, lat: 3 }, geometry: { SkyRegion: { lon: "?" } }, disputes: [{}] });
    expect(detail.shape?.geometry.kind).toBe("sky_region");
    expect(detail.record.geometry).toBeUndefined();
    expect(detail.record.disputes).toBeUndefined();
    expect(skyView(readStructure({ ...vela, record: { ...vela.record, geometry: null, position: { form: "spherical", frame: "galactic", lon: null, lat: null } } }))?.lines).toEqual([]);
  });
});
