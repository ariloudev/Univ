// The Show drawer's structures (choose-what-is-drawn): each kind as a toggle with its colour and
// count, among the kinds of its family.

import { structureCss } from "../../scene/kinds.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { counted, sentence } from "../record/wording.ts";
import { Swatch } from "../Swatch.tsx";
import { words } from "../text.ts";
import { allStructures, noneStructures, onlyKind } from "./actions.ts";
import type { ShowChoices } from "./choices.ts";
import { Caption, Chip, Only, PartHeading } from "./controls.tsx";

export function Structures({ choices, hidden, onApply }: { choices: ShowChoices; hidden: Hidden; onApply: Apply }) {
  return (
    <section className="space-y-1">
      <PartHeading title="Structures" onAll={() => onApply(allStructures(choices))} onNone={() => onApply(noneStructures(choices))} />
      {choices.families.map((f) => (
        <div key={f.family} data-testid={`show-family-${f.family}`}>
          <Caption>{sentence(f.family)}</Caption>
          <div className="mt-0.5 flex flex-wrap gap-1">
            {f.kinds.map(({ kind, count }) => {
              const on = !hidden.structure.has(kind);
              return (
                <span key={kind} className="group inline-flex items-center gap-1">
                  <Chip on={on} name={`${words(kind)}, ${counted(count, "structure")}`} onPress={() => onApply([{ group: "structure", name: kind, shown: !on }])}>
                    <Swatch colour={structureCss(kind)} hollow={!on} />
                    {words(kind)}
                    <span className="text-dim">{count}</span>
                  </Chip>
                  <Only what={words(kind)} onOnly={() => onApply(onlyKind(kind, choices))} />
                </span>
              );
            })}
          </div>
        </div>
      ))}
    </section>
  );
}
