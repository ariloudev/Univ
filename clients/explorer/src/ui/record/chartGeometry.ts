// The geometry of the record panels' charts (read-an-object, see-the-structures): round ticks
// over the values drawn, and the linear map from a value to its place.

/** A step of 1, 2, 2.5 or 5 times a power of ten near `raw`. */
function niceStep(raw: number): number {
  const power = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / power;
  const nice = fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 2.5 ? 2.5 : fraction <= 5 ? 5 : 10;
  return nice * power;
}

/** The round domain about `values` and its ticks, about `count` of them; a single value, or
 * values all alike, are given a domain about them. */
export function niceDomain(values: readonly number[], count = 4): { domain: [number, number]; ticks: number[] } {
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return { domain: [0, 1], ticks: [0, 1] };
  if (hi === lo) {
    const pad = Math.abs(lo) / 10 || 1;
    lo -= pad;
    hi += pad;
  }
  const step = niceStep((hi - lo) / count);
  const from = Math.floor(lo / step) * step;
  const to = Math.ceil(hi / step) * step;
  const ticks: number[] = [];
  for (let t = from; t <= to + step / 1e6; t += step) ticks.push(Number(t.toPrecision(12)));
  return { domain: [from, to], ticks };
}

/** The map from `domain` onto `range`, linearly. */
export const linear =
  ([d0, d1]: readonly [number, number], [r0, r1]: readonly [number, number]) =>
  (v: number): number =>
    r0 + ((v - d0) / (d1 - d0 || 1)) * (r1 - r0);
