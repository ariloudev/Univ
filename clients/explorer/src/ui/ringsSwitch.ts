// Whether the distance rings are drawn (frame-the-view-as-an-atlas): off unless the page's
// address turns them on, and kept there while they are on.

import { useEffect } from "react";

import type { Controller } from "../scene/controller.ts";
import { type AddressSwitch, type Switch, switchInAddress, useAddressSwitch } from "./addressSwitch.ts";

const RINGS: AddressSwitch = { parameter: "rings", byDefault: false };

/** Whether the address `search` turns the distance rings on. */
export const ringsInAddress = (search: string): boolean => switchInAddress(search, RINGS);

/** Whether the distance rings are on, read from the page's address and kept in it, and the way to
 * turn them on or off. */
export const useRings = (): Switch => useAddressSwitch(RINGS);

/** The distance rings' switch, the rings drawn by `controller` while it is on. */
export function useDrawnRings(controller: Controller | undefined): Switch {
  const rings = useRings();
  const [on] = rings;
  useEffect(() => {
    controller?.setRings(on);
  }, [controller, on]);
  return rings;
}
