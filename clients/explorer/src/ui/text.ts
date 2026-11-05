// How the explorer words an object's values (read-an-object): distances in parsec units with
// light-years, uncertainties in the form their source published them, and the names of rules
// and kinds in plain words.

import type { Measurement, ObjectRecord } from "../api/objects.ts";
import { figures, formatDistance } from "../scene/format.ts";
import type { ScenePoint } from "./scenePoint.ts";

/** A slug such as a rule's or a unit's, in words. */
export const words = (slug: string): string => slug.replaceAll(/[-_]/g, " ");

const four = (value: number): string => figures(value, 4);

type Published = Measurement["native"];
type Uncertainty = NonNullable<Published["uncertainty"]>;

/** A value `v` in `unit` with uncertainty `u`, as its source published it. A stated interval is
 * no error, and a limit is no value: both are said as what they are. */
function withUncertainty(v: string, unit: string, u: Uncertainty | undefined): string {
  const minusPlus = `−${four(u?.minus ?? 0)} +${four(u?.plus ?? 0)}`;
  switch (u?.form) {
    case "symmetric":
      return `${v} ± ${four(u.sigma ?? 0)} ${unit}`;
    case "asymmetric":
      return `${v} ${minusPlus} ${unit}`;
    case "interval":
      return `${v} ${unit}, stated to lie within ${minusPlus} (a range, not an error)`;
    case "credible_interval":
      return `${v} ${minusPlus} ${unit} (${four(u.probability ?? 0)} credible)`;
    case "credible_region":
      return `${v} ${unit} (in a ${four(u.probability ?? 0)} credible region)`;
    case "upper_limit":
      return `< ${v} ${unit} (an upper limit)`;
    case "lower_limit":
      return `> ${v} ${unit} (a lower limit)`;
    case "none_reported":
      return `${v} ${unit}, no uncertainty reported`;
    default:
      return `${v} ${unit}`;
  }
}

/** A published value with its unit and uncertainty, as the source gave them. */
export function valueText(value: Published): string {
  return withUncertainty(four(value.value), words(value.unit), value.uncertainty);
}

/** The preferred distance with its uncertainty, which Univ Server gives as a fraction of it. */
export function preferredText(preferred: NonNullable<ObjectRecord["preferred"]>): string {
  const distance = formatDistance(preferred.distance_mpc);
  const fraction = preferred.uncertainty.total;
  if (fraction === null || fraction === undefined) return `${distance}, no uncertainty reported`;
  return `${distance} ± ${figures(fraction * 100, 2)} % (± ${formatDistance(fraction * preferred.distance_mpc)})`;
}

/** Two bands as a colour index names them, with a minus sign: `B−V`. */
export const bandsText = (bands: string): string => bands.replace("-", "−");

/** A colour index with its bands, such as `B−V 0.65`, to the tile's hundredth of a magnitude. */
const colourText = (colour: { index: number; bands: string }): string =>
  `${bandsText(colour.bands)} ${colour.index.toFixed(2).replace("-", "−")}`;

/** What the hover label says of a point: its id, scale domain, distance, the source and kind of
 * value of that distance, and its colour index where it has one. */
export function pointText(point: ScenePoint): string {
  const colour = point.colour === undefined ? "" : ` · ${colourText(point.colour)}`;
  return `${point.id} · ${words(point.domain)} · ${formatDistance(point.distanceMpc)} · ${point.source} · ${point.kind}${colour}`;
}

/** The heading of an object's panel: the name it was found by, else the first name its entries
 * give, else its first designation with its source. */
export function headingOf(record: ObjectRecord, foundAs?: string): string {
  if (foundAs !== undefined) return foundAs;
  const named = record.entries.find((e) => e.name !== null)?.name ?? undefined;
  if (named !== undefined) return named;
  const first = record.designations[0];
  return first === undefined ? `Object ${record.id}` : `${first.identifier} (${first.source})`;
}

/** What the panel says an object is: a galaxy group where it is one, and its scale domain. */
export function natureOf(record: ObjectRecord): string {
  const group = record.kind === "galaxy_group" ? "A galaxy group" : undefined;
  const domain = record.position === null ? undefined : `the ${words(record.position.scale_domain)} domain`;
  if (domain === undefined) return group === undefined ? "Not placed, so in no scale domain" : `${group}, not placed`;
  return group === undefined ? `In ${domain}` : `${group}, in ${domain}`;
}
