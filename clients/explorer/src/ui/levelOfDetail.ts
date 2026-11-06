// Whether level of detail is on (draw-the-catalogues): on unless the page's address turns it off,
// and kept there while it is off.

import { useEffect, useState } from "react";

import { setInAddress } from "./address.ts";

/** The address parameter that turns level of detail off, and its value. */
const PARAMETER = "lod";
const OFF = "off";

/** Whether the address `search` leaves level of detail on. */
export const levelOfDetailInAddress = (search: string): boolean => new URLSearchParams(search).get(PARAMETER) !== OFF;

/** Whether level of detail is on, read from the page's address and kept in it, and the way to
 * turn it on or off. */
export function useLevelOfDetail(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState(() => levelOfDetailInAddress(location.search));
  useEffect(() => setInAddress({ [PARAMETER]: on ? undefined : OFF }), [on]);
  return [on, setOn];
}
