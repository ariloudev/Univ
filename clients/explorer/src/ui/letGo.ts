// Choosing again what is chosen already lets it go, and makes Earth the focus again
// (read-an-object, see-the-structures).

import type { Controller } from "../scene/controller.ts";
import type { Clicked } from "./useWorld.ts";

/** The ids of the object and the structure chosen, if any. */
export interface ChosenIds {
  object: number | undefined;
  structure: string | undefined;
}

/** Whether `found` is what is chosen: the chosen object's point, or the chosen structure's name. */
export const isChosen = (found: Clicked, chosen: ChosenIds): boolean => ("name" in found ? found.name.id === chosen.structure : found.point.id === chosen.object);

/** Lets go of what is chosen, by `close`, and makes Earth the focus again: the view no longer
 * turns and zooms about the place of what was let go. */
export function letGo(close: () => void, controller: Pick<Controller, "returnToEarth"> | undefined): void {
  close();
  controller?.returnToEarth();
}
