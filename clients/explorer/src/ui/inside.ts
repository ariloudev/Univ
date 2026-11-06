// The structure chosen, and whether the view is culled to it (see-the-structures): read from the
// page's address, which names the structure while the view is culled to it, and kept there.

import { useEffect, useState } from "react";

import { setInAddress } from "./address.ts";

/** The address parameter that names the structure the view is culled to. */
const PARAMETER = "inside";

/** The structure the address `search` culls the view to, if any. */
export const insideInAddress = (search: string): string | undefined => new URLSearchParams(search).get(PARAMETER) || undefined;

/** The structure chosen and whether the view is culled to it, and the ways to change them. */
export interface StructureChoice {
  structure: string | undefined;
  inside: boolean;
  /** Chooses the structure `id`, the view culled to it where it was culled to the one before. */
  choose: (id: string) => void;
  /** Chooses no structure, and culls the view no more. */
  close: () => void;
  setInside: (on: boolean) => void;
}

/** The structure chosen and whether the view is culled to it: from the page's address at first,
 * and kept in it. */
export function useStructureChoice(): StructureChoice {
  const [structure, setStructure] = useState(() => insideInAddress(location.search));
  const [inside, setInside] = useState(() => structure !== undefined);
  useEffect(() => setInAddress({ [PARAMETER]: inside ? structure : undefined }), [inside, structure]);
  return {
    structure,
    inside,
    choose: setStructure,
    close: () => {
      setStructure(undefined);
      setInside(false);
    },
    setInside,
  };
}
