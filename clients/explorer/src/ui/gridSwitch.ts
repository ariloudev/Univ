// Whether the grid is drawn (frame-the-view-as-an-atlas): off unless the page's address turns it
// on, and kept there while it is on.

import { type AddressSwitch, type Switch, switchInAddress, useAddressSwitch } from "./addressSwitch.ts";

const GRID: AddressSwitch = { parameter: "grid", byDefault: false };

/** Whether the address `search` turns the grid on. */
export const gridInAddress = (search: string): boolean => switchInAddress(search, GRID);

/** Whether the grid is on, read from the page's address and kept in it, and the way to turn it on
 * or off. */
export const useGrid = (): Switch => useAddressSwitch(GRID);
