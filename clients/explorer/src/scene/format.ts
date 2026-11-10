// Distances as the explorer shows them: the view works in megaparsecs, and a person reads
// parsecs to gigaparsecs, with light-years beside them.

/** Light-years in a parsec (IAU 2015: 1 pc = 648000/π au; 1 ly = 63241.077 au). */
const LY_PER_PC = 3.261_563_777;

/** The parsec units a distance is written in, parsecs in each, largest first. */
export const PC_UNITS: readonly (readonly [number, string])[] = [
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
  let format = FIGURES.get(digits);
  if (format === undefined) {
    format = new Intl.NumberFormat("en-GB", { maximumSignificantDigits: digits });
    FIGURES.set(digits, format);
  }
  return minusSign(format.format(value));
}

/** A number's text with a true minus sign, U+2212, in place of the hyphen its format gives. */
export const minusSign = (text: string): string => text.replace("-", "−");

/** A whole number, rounded, with thousands separated and a true minus sign. */
export const whole = (value: number): string => minusSign(Math.round(value).toLocaleString("en-GB"));

/** A format for each number of significant figures, made once: making one costs more than using it. */
const FIGURES = new Map<number, Intl.NumberFormat>();

/** The unit of `units` that suits `value`: the largest no larger than it, or the smallest. */
function unitOf(value: number, units: readonly (readonly [number, string])[]): { size: number; name: string } {
  const [size, name] = units.find(([unit]) => value >= unit) ?? units[units.length - 1] ?? [1, ""];
  return { size, name };
}

function inUnits(value: number, units: readonly (readonly [number, string])[]): string {
  const { size, name } = unitOf(value, units);
  return `${figures(value / size)} ${name}`;
}

/** The parsec unit that suits a distance given in megaparsecs: its size in parsecs and its name. */
export const pcUnit = (mpc: number): { size: number; name: string } => unitOf(mpc * 1e6, PC_UNITS);

/** A distance given in megaparsecs, in the parsec unit that suits it. */
export function shortDistance(mpc: number): string {
  return inUnits(mpc * 1e6, PC_UNITS);
}

/** A distance given in megaparsecs, in light-years, in the unit that suits it. */
export const lightYears = (mpc: number): string => inUnits(mpc * 1e6 * LY_PER_PC, LY_UNITS);

/** A distance given in megaparsecs, in the parsec unit that suits it, with light-years beside. */
export function formatDistance(mpc: number): string {
  const pc = mpc * 1e6;
  return `${inUnits(pc, PC_UNITS)} (${inUnits(pc * LY_PER_PC, LY_UNITS)})`;
}
