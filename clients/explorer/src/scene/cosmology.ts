// Comoving distance under a release's cosmology (see-the-structures): a structure that publishes
// velocities is drawn at the comoving distance of z = cz/c, as Univ Server places it.

/** The speed of light, km/s. */
export const C_KM_S = 299_792.458;

/** A flat ΛCDM cosmology: its Hubble constant, km/s/Mpc, and its matter density. */
export interface FlatCosmology {
  hubble_constant: number;
  omega_matter: number;
}

/** Intervals of Simpson's rule: far finer than any distance the panel draws needs. */
const STEPS = 256;

/** The comoving distance, Mpc, of redshift `z` under flat ΛCDM `cosmology`: c/H₀ times the
 * integral of 1/E(z′) from 0 to z, by Simpson's rule; nothing for no redshift. */
export function comovingMpc(z: number, cosmology: FlatCosmology): number {
  if (!(z > 0)) return 0;
  const om = cosmology.omega_matter;
  const inverse = (x: number) => 1 / Math.sqrt(om * (1 + x) ** 3 + 1 - om);
  const h = z / STEPS;
  let sum = inverse(0) + inverse(z);
  for (let i = 1; i < STEPS; i += 1) sum += (i % 2 === 1 ? 4 : 2) * inverse(i * h);
  return (C_KM_S / cosmology.hubble_constant) * (sum * h) / 3;
}

/** The comoving distance, Mpc, of a velocity cz, km/s, taken as z = cz/c. */
export const velocityMpc = (cz: number, cosmology: FlatCosmology): number => comovingMpc(cz / C_KM_S, cosmology);
