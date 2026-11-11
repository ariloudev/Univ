// The colour a star is drawn in (draw-the-catalogues): its colour index gives its effective
// temperature along the mean dwarf sequence of Pecaut & Mamajek 2013, and the temperature the
// colour a black body of that temperature shows the CIE 1931 standard observer. A star without a
// colour index is drawn in neutral grey.

import type { ColourBands } from "univ-tile-decoder";

import type { Rgba } from "./drawing.ts";
import { DWARF_SEQUENCE, type DwarfRow } from "./dwarfSequence.ts";
import { POINT_OPACITY, css } from "./kinds.ts";

/** What a colour index gives, as estimated for a main-sequence star. */
export interface StarTemperature {
  /** Effective temperature, kelvin. */
  teff: number;
  /** The spectral class of the sequence's nearest row, such as `G2V`. */
  type: string;
  /** Where the index lies beyond the sequence, which end it was taken at. */
  clamped: "hot" | "cool" | undefined;
}

/** The rows of the sequence with a value in `column`, as far as the column rises with type:
 * interpolation needs a column that only rises. */
function rising(column: 2 | 3): { colour: number; row: DwarfRow }[] {
  const out: { colour: number; row: DwarfRow }[] = [];
  for (const row of DWARF_SEQUENCE) {
    const colour = row[column];
    if (colour === null) continue;
    const last = out.at(-1);
    if (last !== undefined && colour <= last.colour) break;
    out.push({ colour, row });
  }
  return out;
}

const SEQUENCES: Record<ColourBands, { colour: number; row: DwarfRow }[]> = {
  "B-V": rising(2),
  "BP-RP": rising(3),
};

/** The temperature and nearest class that `index`, a colour index in `bands`, gives along the
 * dwarf sequence, interpolated in log temperature; an index beyond the sequence is taken at its
 * end, and says so. */
export function temperatureOf(index: number, bands: ColourBands): StarTemperature {
  const sequence = SEQUENCES[bands];
  const first = sequence[0];
  const last = sequence.at(-1);
  if (first === undefined || last === undefined) throw new Error(`no dwarf sequence for ${bands}`);
  if (index <= first.colour) return { teff: first.row[1], type: first.row[0], clamped: index < first.colour ? "hot" : undefined };
  if (index >= last.colour) return { teff: last.row[1], type: last.row[0], clamped: index > last.colour ? "cool" : undefined };
  const above = sequence.findIndex((s) => s.colour >= index);
  const hi = sequence[above];
  const lo = sequence[above - 1];
  if (hi === undefined || lo === undefined) throw new Error("the sequence does not bracket the index");
  const t = (index - lo.colour) / (hi.colour - lo.colour);
  const teff = Math.exp(Math.log(lo.row[1]) + t * (Math.log(hi.row[1]) - Math.log(lo.row[1])));
  return { teff, type: (t < 0.5 ? lo : hi).row[0], clamped: undefined };
}

/** A piecewise Gaussian, each side with its own inverse width: Wyman, Sloan & Shirley 2013,
 * "Simple Analytic Approximations to the CIE XYZ Color Matching Functions", JCGT 2(2), 1–11,
 * eq. 4 and Table 1. */
const lobe = (wave: number, alpha: number, mean: number, below: number, above: number): number => {
  const t = (wave - mean) * (wave < mean ? below : above);
  return alpha * Math.exp(-0.5 * t * t);
};

/** The CIE 1931 2° colour-matching functions at `wave` nanometres, by Wyman et al.'s fit. */
function matching(wave: number): [number, number, number] {
  return [
    lobe(wave, 0.362, 442.0, 0.0624, 0.0374) + lobe(wave, 1.056, 599.8, 0.0264, 0.0323) + lobe(wave, -0.065, 501.1, 0.049, 0.0382),
    lobe(wave, 0.821, 568.8, 0.0213, 0.0247) + lobe(wave, 0.286, 530.9, 0.0613, 0.0322),
    lobe(wave, 1.217, 437.0, 0.0845, 0.0278) + lobe(wave, 0.681, 459.0, 0.0385, 0.0725),
  ];
}

/** hc/k, in nanometre kelvin (CODATA 2018 exact constants). */
const HC_OVER_K = 1.438_776_877e7;

/** Planck's spectral radiance at `wave` nanometres and `teff` kelvin, up to a constant. */
const planck = (wave: number, teff: number): number => 1 / (wave ** 5 * Math.expm1(HC_OVER_K / (wave * teff)));

/** XYZ to linear sRGB (IEC 61966-2-1:1999, D65 white). */
const TO_SRGB = [
  [3.2406, -1.5372, -0.4986],
  [-0.9689, 1.8758, 0.0415],
  [0.0557, -0.204, 1.057],
] as const;

/** The sRGB transfer function (IEC 61966-2-1:1999). */
const encode = (c: number): number => (c <= 0.003_130_8 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

/** The colour, red, green and blue each 0 to 1, of a black body at `teff` kelvin as the CIE 1931
 * observer sees it in sRGB, scaled so that its brightest channel is full: hue, not brightness. */
export function blackBodyColour(teff: number): [number, number, number] {
  let x = 0;
  let y = 0;
  let z = 0;
  for (let wave = 380; wave <= 780; wave += 5) {
    const b = planck(wave, teff);
    const [xb, yb, zb] = matching(wave);
    x += b * xb;
    y += b * yb;
    z += b * zb;
  }
  const linear = TO_SRGB.map(([r, g, b]) => Math.max(0, r * x + g * y + b * z));
  const top = Math.max(...linear);
  const [r, g, b] = linear.map((c) => encode(c / top));
  return [r ?? 0, g ?? 0, b ?? 0];
}

/** The colour of a star without a colour index: neutral grey. */
export const NO_COLOUR_GREY: Rgba = [0.6, 0.6, 0.6, POINT_OPACITY];

/** The temperatures the key labels, kelvin, coolest first. */
export const KEY_TEMPERATURES: readonly number[] = [2500, 3500, 5000, 6500, 10_000, 30_000];

/** The stars' colours at the key's temperatures, coolest first, for CSS: each stop spaced evenly,
 * as the labels below them are, so that each label stands under its colour. */
export const STAR_GRADIENT = `linear-gradient(to right, ${KEY_TEMPERATURES.map((t) => css(blackBodyColour(t))).join(", ")})`;

/** The grey of stars without a colour index, for CSS. */
export const GREY_CSS = css(NO_COLOUR_GREY);

/** A star's colour index, as a tile gives it. */
export type StarColourIndex = { index: number; bands: ColourBands };

/** A colour index as one number: its bands and its hundredths of a magnitude, as tiles hold it. */
const keyOf = ({ index, bands }: StarColourIndex): number => (bands === "B-V" ? 65_536 : 0) + Math.round(index * 100) + 32_768;

const colours = new Map<number, Rgba>();
const bytes = new Map<number, Uint8Array>();

/** The colour a star with `colour`, its colour index and bands, is drawn in; grey without one. */
export function starColour(colour: StarColourIndex | undefined): Rgba {
  if (colour === undefined) return NO_COLOUR_GREY;
  const key = keyOf(colour);
  const known = colours.get(key);
  if (known !== undefined) return known;
  const [r, g, b] = blackBodyColour(temperatureOf(colour.index, colour.bands).teff);
  const rgba: Rgba = [r, g, b, POINT_OPACITY];
  colours.set(key, rgba);
  return rgba;
}

/** A colour as red, green, blue and opacity bytes, as the renderer takes it. */
export const toBytes = (rgba: Rgba): Uint8Array => Uint8Array.from(rgba, (c) => Math.round(c * 255));
const GREY_BYTES = toBytes(NO_COLOUR_GREY);

/** `starColour` as red, green, blue and opacity bytes, as the renderer takes them; computed once
 * for each colour index, as the tiles give it to the hundredth of a magnitude. */
export function starBytes(colour: StarColourIndex | undefined): Uint8Array {
  if (colour === undefined) return GREY_BYTES;
  const key = keyOf(colour);
  const known = bytes.get(key);
  if (known !== undefined) return known;
  const made = toBytes(starColour(colour));
  bytes.set(key, made);
  return made;
}
