// The Show drawer's structures (choose-what-is-drawn): each kind as a toggle with its colour and
// count, among the kinds of its family.

import { structureCss } from "../../scene/kinds.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { Note } from "../record/parts.tsx";
import { counted, sentence } from "../record/wording.ts";
import { Swatch } from "../Swatch.tsx";
import { words } from "../text.ts";
import { allStructures, noneStructures, onlyKind } from "./actions.ts";
import type { ShowChoices } from "./choices.ts";
import { GroupChip, Only, Part } from "./controls.tsx";

export function Structures({ choices, hidden, onApply }: { choices: ShowChoices; hidden: Hidden; onApply: Apply }) {
  return (
    <Part title="Structures" onAll={() => onApply(allStructures(choices))} onNone={() => onApply(noneStructures(choices))}>
      {choices.families.map((f) => (
        <div key={f.family} data-testid={`show-family-${f.family}`}>
          <Note>{sentence(f.family)}</Note>
          <div className="mt-0.5 flex flex-wrap gap-1">
            {f.kinds.map(({ kind, count }) => (
              <span key={kind} className="group relative inline-flex">
                <GroupChip group="structure" name={kind} label={`${words(kind)} ${counted(count, "structure")}`} hidden={hidden} onApply={onApply}>
                  {(on) => (
                    <>
                      <Swatch colour={structureCss(kind)} hollow={!on} small />
                      {words(kind)} <span className="text-dim">{count}</span>
                    </>
                  )}
                </GroupChip>
                <Only what={words(kind)} onOnly={() => onApply(onlyKind(kind, choices))} place="beside" />
              </span>
            ))}
          </div>
        </div>
      ))}
    </Part>
  );
}
