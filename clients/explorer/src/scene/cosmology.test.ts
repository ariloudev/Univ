import { describe, expect, it } from "vitest";

import { comovingMpc, velocityMpc } from "./cosmology.ts";

const PLANCK_2018 = { hubble_constant: 67.66, omega_matter: 0.3111 };

describe("comoving distance", () => {
  it("gives the comoving distance of z = cz/c under flat ΛCDM, as Univ Server places a structure's velocities", () => {
    // Vela's near and far bounds and walls, cz 16,000, 18,000, 22,000 and 24,000 km/s.
    expect(velocityMpc(16_000, PLANCK_2018)).toBeCloseTo(233.5, 1);
    expect(velocityMpc(18_000, PLANCK_2018)).toBeCloseTo(262.3, 1);
    expect(velocityMpc(24_000, PLANCK_2018)).toBeCloseTo(348.0, 1);
    // Far out, the integral bends well below the Hubble law: z = 1.26, about 4.0 Gpc.
    expect(comovingMpc(1.259759, PLANCK_2018)).toBeCloseTo(4000, -1);
    expect(comovingMpc(0, PLANCK_2018)).toBe(0);
    expect(comovingMpc(-0.001, PLANCK_2018)).toBe(0);
  });
});
