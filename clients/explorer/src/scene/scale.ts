// The scale of the view (frame-the-view-as-an-atlas): a bar of a round length at the distance
// of the focus, as an atlas gives the scale of its chart.

/** The shortest and longest the bar is drawn, CSS pixels. */
export const SHORTEST = 60;
export const LONGEST = 160;

const UNITS: readonly (readonly [number, string])[] = [
  [1e6, "Mpc"],
  [1e3, "kpc"],
  [1, "pc"],
];

/** The bar for a view where one CSS pixel spans `mpcPerPixel` megaparsecs at the focus: the
 * shortest length of 1, 2 or 5 times a power of ten of parsecs that is at least `SHORTEST`
 * pixels long (each such length is at most 2.5 times the one before, so it is shorter than
 * `LONGEST`), labelled in parsecs, kiloparsecs or megaparsecs. */
export function scaleBar(mpcPerPixel: number): { pixels: number; parsecs: number; label: string } {
  const pcPerPixel = mpcPerPixel * 1e6;
  const least = SHORTEST * pcPerPixel;
  let power = 10 ** Math.floor(Math.log10(least));
  for (;;) {
    for (const m of [1, 2, 5]) {
      const parsecs = m * power;
      if (parsecs >= least * (1 - 1e-9)) {
        const [size, unit] = UNITS.find(([u]) => parsecs >= u) ?? UNITS[UNITS.length - 1] ?? [1, "pc"];
        const value = Number((parsecs / size).toPrecision(1));
        return { pixels: parsecs / pcPerPixel, parsecs, label: `${value.toLocaleString("en-GB", { maximumFractionDigits: 12 })} ${unit}` };
      }
    }
    power *= 10;
  }
}
