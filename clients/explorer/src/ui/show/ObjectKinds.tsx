// The Show drawer's kinds of object of the Galaxy's tracers (choose-what-is-drawn): the switch of
// their natural colours (draw-the-catalogues), then a switch for each, in the colour its points
// are drawn in, with the number of its points drawn as the key gives it.

import type { TracerLine } from "../../catalogue/catalogue.ts";
import { css } from "../../scene/kinds.ts";
import type { Switch } from "../addressSwitch.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { Note } from "../record/parts.tsx";
import { type Store, useStore } from "../store.ts";
import { Swatch } from "../Swatch.tsx";
import { SwitchBox } from "../SwitchBox.tsx";
import { GroupChip } from "./controls.tsx";

/** The Galaxy's tracers drawn by kind of object, read again with each frame drawn, and whether
 * they are drawn in their natural colours. */
export interface TracerKinds {
  lines: () => TracerLine[];
  frame: Store<number>;
  natural: Switch;
}

/** A switch for each kind of object of the tracers, with its colour and its points drawn; none
 * for the tracers of unknown kind, which no switch turns off. */
export function ObjectKinds({ tracers, hidden, onApply }: { tracers: TracerKinds; hidden: Hidden; onApply: Apply }) {
  useStore(tracers.frame);
  const lines = tracers.lines().filter((line) => line.id !== "unknown");
  return (
    <>
      <Note>The Galaxy's tracers by kind of object, with their points drawn</Note>
      <SwitchBox label="Natural colours" state={tracers.natural} textClass="text-sm" />
      <div className="grid grid-cols-2 gap-1" data-testid="show-objects">
        {lines.map((line) => {
          const points = line.points.toLocaleString("en-GB");
          return (
            // Named with its count, as it shows it.
            <GroupChip key={line.id} group="object" name={line.id} label={`${line.title} ${points} drawn`} hidden={hidden} onApply={onApply}>
              {(on) => (
                <>
                  <Swatch colour={css(line.colour)} hollow={!on} small />
                  <span className="truncate">{line.title}</span>{" "}
                  <span className="ml-auto text-dim">{points}</span>
                </>
              )}
            </GroupChip>
          );
        })}
      </div>
    </>
  );
}
