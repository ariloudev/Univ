// How the explorer words an object's values (read-an-object): distances in parsec units with
// light-years, uncertainties as published, and the names of rules and kinds in plain words.

import type { Measurement, ObjectRecord } from "../api/objects.ts";
import { formatDistance } from "../scene/format.ts";

/** A slug such as a rule's or a unit's, in words. */
export const words = (slug: string): string => slug.replaceAll(/[-_]/g, " ");

const figures = (value: number): string => Number(value.toPrecision(4)).toLocaleString("en-GB");

/** A published value with its unit and uncertainty, as the source gave it. */
export function valueText(value: Measurement["native"]): string {
  const u = value.uncertainty;
  let spread = "";
  if (u?.sigma !== undefined) spread = ` ± ${figures(u.sigma)}`;
  else if (u?.minus !== undefined && u.plus !== undefined) spread = ` −${figures(u.minus)} +${figures(u.plus)}`;
  else if (u?.form === "none_reported") spread = ", no uncertainty reported";
  return `${figures(value.value)} ${words(value.unit)}${spread}`;
}

/** The preferred distance with its uncertainty, which Univ Server gives as a fraction of it. */
export function preferredText(preferred: NonNullable<ObjectRecord["preferred"]>): string {
  const distance = formatDistance(preferred.distance_mpc);
  const fraction = preferred.uncertainty.total;
  if (fraction === null || fraction === undefined) return `${distance}, no uncertainty reported`;
  const percent = Number((fraction * 100).toPrecision(2));
  return `${distance} ± ${percent} % (± ${formatDistance(fraction * preferred.distance_mpc)})`;
}

/** What the hover label says of a point: its id, scale domain, distance, and the source and
 * kind of value of that distance. */
export function pointText(point: { id: number; domain: string; distanceMpc: number; source: string; kind: string }): string {
  return `${point.id} · ${words(point.domain)} · ${formatDistance(point.distanceMpc)} · ${point.source} · ${point.kind}`;
}
