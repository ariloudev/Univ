// The scale of the view (frame-the-view-as-an-atlas): a bar of a round length at the distance
// of the focus, as an atlas gives the scale of its chart.

/** The shortest and longest the bar is drawn, CSS pixels. */
export const SHORTEST = 60;
export const LONGEST = 160;

/** The units the bar is labelled in: parsecs, kiloparsecs or megaparsecs, as the scale's
 * criterion names them; unlike a distance, never gigaparsecs. */
const UNITS: readonly (readonly [number, string])[] = [
  [1e6, "Mpc"],
  [1e3, "kpc"],
  [1, "pc"],
];

const FORMAT = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 12 });

/** The bar for a view where one CSS pixel spans `mpcPerPixel` megaparsecs at the focus: the
 * shortest length of 1, 2 or 5 times a power of ten of parsecs that is at least `SHORTEST`
 * pixels long. Each such length is at most 2.5 times the one before, so it is shorter than
 * `LONGEST`. */
export function scaleBar(mpcPerPixel: number): { pixels: number; parsecs: number; label: string } | undefined {
  if (!Number.isFinite(mpcPerPixel) || mpcPerPixel <= 0) return undefined;
  const pcPerPixel = mpcPerPixel * 1e6;
  const least = SHORTEST * pcPerPixel;
  const power = 10 ** Math.floor(Math.log10(least));
  const parsecs = [1, 2, 5, 10].map((m) => m * power).find((p) => p >= least * (1 - 1e-9)) ?? 10 * power;
  const [size, unit] = UNITS.find(([u]) => parsecs >= u) ?? [1, "pc"];
  return { pixels: parsecs / pcPerPixel, parsecs, label: `${FORMAT.format(Number((parsecs / size).toPrecision(1)))} ${unit}` };
}
