// Whether the structures' outlines are drawn (see-the-structures): on unless the page's address
// turns them off, and kept there while they are off. Their names stay either way.

import { type AddressSwitch, type Switch, switchInAddress, useAddressSwitch } from "./addressSwitch.ts";

const OUTLINES: AddressSwitch = { parameter: "outlines", byDefault: true };

/** Whether the address `search` leaves the structures' outlines on. */
export const outlinesInAddress = (search: string): boolean => switchInAddress(search, OUTLINES);

/** Whether the outlines are on, read from the page's address and kept in it, and the way to turn
 * them on or off. */
export const useOutlines = (): Switch => useAddressSwitch(OUTLINES);
