// Whether the structures' outlines are drawn (see-the-structures): on unless the page's address
// turns them off, and kept there while they are off. Their names stay either way.

import { useEffect } from "react";

import { type AddressSwitch, type Switch, switchInAddress, useAddressSwitch } from "./addressSwitch.ts";
import type { World } from "./useWorld.ts";

const OUTLINES: AddressSwitch = { parameter: "outlines", byDefault: true };

/** Whether the address `search` leaves the structures' outlines on. */
export const outlinesInAddress = (search: string): boolean => switchInAddress(search, OUTLINES);

/** The outlines' switch, read from the page's address and kept in it, the outlines drawn by
 * `world` while it is on. */
export function useDrawnOutlines(world: World | undefined): Switch {
  const outlines = useAddressSwitch(OUTLINES);
  const [on] = outlines;
  useEffect(() => {
    if (world === undefined) return;
    world.renderer.showOutlines(on);
    world.controller.invalidate();
  }, [world, on]);
  return outlines;
}
