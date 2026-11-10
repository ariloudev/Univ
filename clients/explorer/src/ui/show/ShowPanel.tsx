// The Show drawer (choose-what-is-drawn): a summary of what is drawn with one way back to all of
// it, then the points, by kind of value and by source and scale domain, and the structures, by
// kind within their families.

import type { Apply, Hidden } from "../hidden.ts";
import { Chooser } from "../record/parts.tsx";
import { SwitchBox } from "../SwitchBox.tsx";
import type { DustState } from "../useDustMap.ts";
import { everything } from "./actions.ts";
import { type ShowChoices, type Tally, anyOff, summaryOf } from "./choices.ts";
import { Points } from "./Points.tsx";
import { Structures } from "./Structures.tsx";

export interface ShowPanelProps {
  choices: ShowChoices;
  hidden: Hidden;
  onApply: Apply;
  /** The dust map's switch, where the view has one (draw-the-dust-map). */
  dust?: DustState | undefined;
}

const of = ({ on, of: all }: Tally, what: string) => `${on} of ${all} ${what}`;

export function ShowPanel({ choices, hidden, onApply, dust }: ShowPanelProps) {
  const sum = summaryOf(choices, hidden);
  return (
    <div className="space-y-3">
      <div>
        <h2 className="font-serif text-base">Show</h2>
        <p className="text-xs text-dim" data-testid="show-summary">
          Points: {of(sum.domains, "domains")}, {of(sum.sources, "catalogues")}, {of(sum.kinds, "kinds")}. Structures: {of(sum.structures, "kinds")}.
          {anyOff(sum) ? (
            <>
              {" "}
              <Chooser onClick={() => onApply(everything(hidden))}>Show everything</Chooser>
            </>
          ) : null}
        </p>
      </div>
      <div data-testid="show" className="space-y-3">
        <Points choices={choices} hidden={hidden} onApply={onApply} />
        <Structures choices={choices} hidden={hidden} onApply={onApply} />
        {dust === undefined ? null : <DustSwitch dust={dust} />}
      </div>
    </div>
  );
}

/** The dust map's switch, and why it turned itself off, if it did. */
function DustSwitch({ dust }: { dust: DustState }) {
  const [on] = dust.state;
  return (
    <section data-testid="dust-switch" className="space-y-1">
      <h3 className="font-semibold">Dust</h3>
      <SwitchBox label="Dust map" state={dust.state} textClass="text-sm" />
      {on && dust.map === undefined ? <p className="text-xs text-dim">Reading the dust map…</p> : null}
      {dust.problem === undefined ? null : (
        <p role="status" className="text-xs text-warn">
          {dust.problem}
        </p>
      )}
    </section>
  );
}
