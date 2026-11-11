// The Show drawer's kinds of object of the Galaxy's tracers (choose-what-is-drawn): a switch for
// each, in the colour its points are drawn in, with the number of its points drawn as the key
// gives it.

import type { TracerLine } from "../../catalogue/catalogue.ts";
import { css } from "../../scene/kinds.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { Note } from "../record/parts.tsx";
import { type Store, useStore } from "../store.ts";
import { Swatch } from "../Swatch.tsx";
import { GroupChip } from "./controls.tsx";

/** The Galaxy's tracers drawn by kind of object, read again with each frame drawn. */
export interface TracerKinds {
  lines: () => TracerLine[];
  frame: Store<number>;
}

/** A switch for each kind of object of `kinds`, with its colour and its points drawn. */
export function ObjectKinds({ kinds, tracers, hidden, onApply }: { kinds: readonly string[]; tracers: TracerKinds; hidden: Hidden; onApply: Apply }) {
  useStore(tracers.frame);
  const lines = new Map(tracers.lines().map((line) => [line.id, line]));
  return (
    <>
      <Note>The Galaxy's tracers by kind of object, with their points drawn</Note>
      <div className="grid grid-cols-2 gap-1" data-testid="show-objects">
        {kinds.map((kind) => {
          const line = lines.get(kind);
          if (line === undefined) return null;
          return (
            <GroupChip key={kind} group="object" name={kind} label={line.title} hidden={hidden} onApply={onApply}>
              {(on) => (
                <>
                  <Swatch colour={css(line.colour)} hollow={!on} small />
                  <span className="truncate">{line.title}</span>
                  <span className="ml-auto text-dim">{line.points.toLocaleString("en-GB")}</span>
                </>
              )}
            </GroupChip>
          );
        })}
      </div>
    </>
  );
}
