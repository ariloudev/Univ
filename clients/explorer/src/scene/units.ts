// The length units Univ Server places things in, in megaparsecs. A unit not listed cannot be
// placed: `megaparsec_over_h` needs the release's h, and the explorer is not given it.

const MPC_PER = { parsec: 1e-6, kiloparsec: 1e-3, megaparsec: 1 } as const;

type LengthUnit = keyof typeof MPC_PER;

/** The megaparsecs in one `unit`; none for a unit the explorer cannot place. */
export function toMpc(unit: string): number | undefined {
  return Object.hasOwn(MPC_PER, unit) ? MPC_PER[unit as LengthUnit] : undefined;
}
