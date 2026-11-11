// The age below which the Galaxy's tracers are drawn (choose-what-is-drawn): any age, or younger
// than 100, 30 or 10 million years, with the share of the tracers held that carry an age, so that
// the reader knows which it acts on.

import { AGE_CUTS, type AgeCut, type AgesHeld } from "../age.ts";
import { Note } from "../record/parts.tsx";
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
        {OPTIONS.map(({ cut, label }) => {
          const on = cut === age.cut;
          return (
            <button
              key={label}
              type="button"
              aria-pressed={on}
              aria-label={cut === undefined ? "Tracers of any age" : `Tracers younger than ${cut} million years`}
              onClick={() => age.choose(cut)}
              className={`rounded-full border px-2 py-px text-xs ${on ? "border-edge bg-raised" : "border-dashed border-neat/60 text-dim"}`}
            >
              {label}
            </button>
          );
        })}
      </div>
      <p data-testid="show-age-share" className="text-xs text-dim">
        {aged.toLocaleString("en-GB")} of the {tracers.toLocaleString("en-GB")} tracers held carry an age: the choice acts on
        them alone, and the others are drawn as their switches say.
      </p>
    </div>
  );
}
