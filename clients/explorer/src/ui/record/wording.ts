// How the record panels word their values in short (read-an-object, see-the-structures): a value
// with its uncertainty in the form its source published it, kept short; a sentence's first
// letter; an article; a count with its noun.

import type { Measurement } from "../../api/objects.ts";
import { figures } from "../../scene/format.ts";

/** A published uncertainty, in the form its source gives it. */
export type Uncertainty = NonNullable<Measurement["native"]["uncertainty"]>;

/** How far below and above its value an uncertainty runs, where it is a spread about it: an error,
 * a credible interval, or a stated range. None for a limit, or where none is reported. */
export function spread(u: Uncertainty | undefined): [number, number] | undefined {
  switch (u?.form) {
    case "symmetric":
      return [u.sigma ?? 0, u.sigma ?? 0];
    case "asymmetric":
    case "credible_interval":
    case "interval":
      return u.minus === undefined || u.plus === undefined ? undefined : [u.minus, u.plus];
    default:
      return undefined;
  }
}

/** The value `v`, already worded, with uncertainty `u` in short: `± σ`, `+p −m`, a stated range
 * as one, and a limit as `<` or `≥`; scaled by `k` where the value was converted, such as from z to
 * cz. Figures of the uncertainty are two. */
export function shortWithUncertainty(v: string, u: Uncertainty | undefined, k = 1): string {
  const two = (x: number) => figures(x * k, 2);
  switch (u?.form) {
    case "symmetric":
      return `${v} ± ${two(u.sigma ?? 0)}`;
    case "asymmetric":
    case "credible_interval":
      return `${v} +${two(u.plus ?? 0)} −${two(u.minus ?? 0)}`;
    case "interval":
      return `${v} (within −${two(u.minus ?? 0)} +${two(u.plus ?? 0)})`;
    case "upper_limit":
      return `< ${v}`;
    case "lower_limit":
      return `≥ ${v}`;
    default:
      return v;
  }
}

/** `text` with its first letter capitalised, as a sentence or a label begins. */
export const sentence = (text: string): string => text.charAt(0).toUpperCase() + text.slice(1);

/** `noun` with its article: "an" before a vowel sound, a capital letter such as F named included. */
/** `noun` after "a" or "an" by its sound: "an" before a vowel, a letter named with one ("an
 * F-type star") or "RR" ("an RR Lyrae star"). */
export const withArticle = (noun: string): string => `${/^([aeiou]|[AEFHILMNORSX]-|RR )/i.test(noun) ? "an" : "a"} ${noun}`;

/** `n` and its noun, singular for one. */
export const counted = (n: number, singular: string, plural = `${singular}s`): string => `${n.toLocaleString("en-GB")} ${n === 1 ? singular : plural}`;
