// Choosing again what is chosen already lets it go (read-an-object, see-the-structures): by a
// click in the view, a little after it, unless a double-click follows, which keeps it chosen; or
// at once from the list of structures. Letting go closes its record and makes Earth the focus
// again.

import type { Clicked } from "./useWorld.ts";

/** What a click or the list names: an object by its id, or a structure by its id. */
export type Target = { object: number } | { structure: string };

/** What `found`, under a click in the view, names. */
export const targetOf = (found: Clicked): Target => ("name" in found ? { structure: found.name.id } : { object: found.point.id });

/** Whether `target` is what is chosen: the object chosen, or the structure chosen. */
export const isChosen = (target: Target, chosen: { object: number | undefined; structure: string | undefined }): boolean =>
  "structure" in target ? target.structure === chosen.structure : target.object === chosen.object;

/** The milliseconds a click on what is chosen waits before it lets it go: the first click of a
 * double-click, which keeps it chosen, lets nothing go. */
export const LET_GO_AFTER_MS = 300;

/** What the view's clicks do with what they find: a double-click chooses it; a click chooses it,
 * or, where it is chosen already, lets it go once `LET_GO_AFTER_MS` pass with no double-click. */
export function clickChooser<T>(on: { isChosen: (found: T) => boolean; letGo: () => void; choose: (found: T, double: boolean) => void }, wait = LET_GO_AFTER_MS) {
  let pending: ReturnType<typeof setTimeout> | undefined;
  const cancel = () => {
    if (pending !== undefined) clearTimeout(pending);
    pending = undefined;
  };
  return {
    click(found: T): void {
      cancel();
      if (!on.isChosen(found)) {
        on.choose(found, false);
        return;
      }
      pending = setTimeout(() => {
        pending = undefined;
        on.letGo();
      }, wait);
    },
    doubleClick(found: T): void {
      cancel();
      on.choose(found, true);
    },
    /** Lets nothing go that a click asked for. */
    cancel,
  };
}
