// Distances as the explorer shows them: the view works in megaparsecs, and a person reads
// parsecs to gigaparsecs, with light-years beside them.

/** Light-years in a parsec (IAU 2015: 1 pc = 648000/π au; 1 ly = 63241.077 au). */
const LY_PER_PC = 3.261_563_777;

const PC_UNITS: readonly (readonly [number, string])[] = [
  [1e9, "Gpc"],
  [1e6, "Mpc"],
  [1e3, "kpc"],
  [1, "pc"],
];

const LY_UNITS: readonly (readonly [number, string])[] = [
  [1e9, "billion ly"],
  [1e6, "million ly"],
  [1e3, "thousand ly"],
  [1, "ly"],
];

/** `value` to `digits` significant figures, without a trailing zero after the point. */
export function figures(value: number, digits = 3): string {
  return value.toLocaleString("en-GB", { maximumSignificantDigits: digits });
}

function inUnits(value: number, units: readonly (readonly [number, string])[]): string {
  const [size, name] = units.find(([unit]) => value >= unit) ?? units[units.length - 1] ?? [1, ""];
  return `${figures(value / size)} ${name}`;
}

/** A distance given in megaparsecs, in the parsec unit that suits it. */
export function shortDistance(mpc: number): string {
  return inUnits(mpc * 1e6, PC_UNITS);
}

/** A distance given in megaparsecs, in the parsec unit that suits it, with light-years beside. */
export function formatDistance(mpc: number): string {
  const pc = mpc * 1e6;
  return `${inUnits(pc, PC_UNITS)} (${inUnits(pc * LY_PER_PC, LY_UNITS)})`;
}
