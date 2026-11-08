// Whether level of detail is on (draw-the-catalogues): on unless the page's address turns it off,
// and kept there while it is off.

import { type AddressSwitch, switchInAddress, useAddressSwitch } from "./addressSwitch.ts";

const LEVEL_OF_DETAIL: AddressSwitch = { parameter: "lod", byDefault: true };

/** Whether the address `search` leaves level of detail on. */
export const levelOfDetailInAddress = (search: string): boolean => switchInAddress(search, LEVEL_OF_DETAIL);

/** Whether level of detail is on, read from the page's address and kept in it, and the way to
 * turn it on or off. */
export const useLevelOfDetail = (): [boolean, (on: boolean) => void] => useAddressSwitch(LEVEL_OF_DETAIL);
