// The age below which the Galaxy's tracers are drawn (choose-what-is-drawn): any age, or younger
// than 100, 30 or 10 million years, kept in the page's address as its `age` parameter. Young open
// clusters trace the spiral arms (Hunt & Reffert 2023's ages), which the clusters of every age
// fill in.

import { useCallback, useEffect, useState } from "react";

import { setInAddress } from "./address.ts";

/** The ages offered, in millions of years, oldest first. */
export const AGE_CUTS = [100, 30, 10] as const;

/** An age offered, in millions of years; none for any age. */
export type AgeCut = (typeof AGE_CUTS)[number] | undefined;

/** The age the page's address chooses: `age=30`; any age where it names none offered. */
export function ageInAddress(search: string): AgeCut {
  const value = Number(new URLSearchParams(search).get("age"));
  return AGE_CUTS.find((cut) => cut === value);
}

/** The base-10 logarithm, in years, at or above which a tracer is not drawn under `cut`; none for
 * any age. */
export const logAgeOf = (cut: AgeCut): number | undefined => (cut === undefined ? undefined : Math.log10(cut * 1e6));

/** The age chosen, read from the page's address and kept in it, and the way to choose another. */
export function useAgeCut(): [AgeCut, (cut: AgeCut) => void] {
  const [cut, setCut] = useState<AgeCut>(() => ageInAddress(location.search));
  useEffect(() => setInAddress({ age: cut === undefined ? undefined : String(cut) }), [cut]);
  const choose = useCallback((next: AgeCut) => setCut(next), []);
  return [cut, choose];
}

/** The Galaxy's tracers held, and of them those that carry an age. */
export interface AgesHeld {
  tracers: number;
  aged: number;
}
