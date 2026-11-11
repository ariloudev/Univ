// The age below which the Galaxy's tracers are drawn (choose-what-is-drawn): any age, or younger
// than 100, 30 or 10 million years, with the share of the tracers held that carry an age, so that
// the reader knows which it acts on.

import type { AgesHeld } from "../../catalogue/catalogue.ts";
import { AGE_CUTS, type AgeCut } from "../age.ts";
import { Note } from "../record/parts.tsx";
import { Chip } from "./controls.tsx";
import { type Store, useStore } from "../store.ts";

/** The age chosen, the way to choose another, and the tracers held, read as each frame is drawn. */
export interface AgeState {
  cut: AgeCut;
  choose: (cut: AgeCut) => void;
  held: () => AgesHeld;
  frame: Store<number>;
}

const OPTIONS: readonly { cut: AgeCut; label: string }[] = [
  { cut: undefined, label: "any" },
  ...AGE_CUTS.map((cut) => ({ cut, label: `< ${cut}` })),
];

/** One button for each age offered, the one chosen pressed. */
export function AgeChoice({ age }: { age: AgeState }) {
  useStore(age.frame);
  const { tracers, aged } = age.held();
  return (
    <div data-testid="show-age" className="space-y-1">
      <Note>The Galaxy's tracers by age, in millions of years</Note>
      <div role="group" aria-label="The Galaxy's tracers by age" className="flex gap-1">
        {OPTIONS.map(({ cut, label }) => (
          <Chip
            key={label}
            on={cut === age.cut}
            label={cut === undefined ? "Tracers of any age" : `Tracers younger than ${cut} million years`}
            onClick={() => age.choose(cut)}
          >
            {label}
          </Chip>
        ))}
      </div>
      <p data-testid="show-age-share" className="text-xs text-dim">
        {aged.toLocaleString("en-GB")} of the {tracers.toLocaleString("en-GB")} tracers held carry an age: the choice acts on
        them alone, and the others are drawn as their switches say.
      </p>
    </div>
  );
}
