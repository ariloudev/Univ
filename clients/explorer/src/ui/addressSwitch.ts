// A switch kept in the page's address (draw-the-catalogues, frame-the-view-as-an-atlas): as it
// starts unless the address holds it the other way, and kept there while it is.

import { useEffect, useState } from "react";

import { setInAddress } from "./address.ts";

/** A switch's address parameter, and whether it is on unless the address says otherwise. */
export interface AddressSwitch {
  parameter: string;
  byDefault: boolean;
}

/** The value the address holds for a switch turned from its default. */
const turned = ({ byDefault }: AddressSwitch): string => (byDefault ? "off" : "on");

/** Whether the address `search` leaves the switch on. */
export const switchInAddress = (search: string, which: AddressSwitch): boolean =>
  new URLSearchParams(search).get(which.parameter) === turned(which) ? !which.byDefault : which.byDefault;

/** Whether the switch is on, read from the page's address and kept in it while it is turned from
 * its default, and the way to turn it on or off. */
export function useAddressSwitch(which: AddressSwitch): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState(() => switchInAddress(location.search, which));
  const { parameter, byDefault } = which;
  useEffect(() => setInAddress({ [parameter]: on === byDefault ? undefined : turned({ parameter, byDefault }) }), [on, parameter, byDefault]);
  return [on, setOn];
}
