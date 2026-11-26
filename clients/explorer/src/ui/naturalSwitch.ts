// Whether the Galaxy tracers are drawn in their kinds' natural colours (draw-the-catalogues): on
// unless the page's address turns them off, and kept there while they are off.

import { type AddressSwitch, type Switch, useAddressSwitch } from "./addressSwitch.ts";

const NATURAL: AddressSwitch = { parameter: "natural", byDefault: true };

/** Whether natural colours are on, read from the page's address and kept in it, and the way to
 * turn them on or off. */
export const useNaturalColours = (): Switch => useAddressSwitch(NATURAL);
