// The Show drawer's points (choose-what-is-drawn): the kinds of value as toggles with the key's
// marks, and one table of the sources, each once, with a column for each scale domain.

import { KINDS, POINT_CSS } from "../../scene/kinds.ts";
import { figures } from "../../scene/format.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { Mark } from "../Mark.tsx";
import { Note } from "../record/parts.tsx";
import { sentence } from "../record/wording.ts";
import { words } from "../text.ts";
import { allPoints, nonePoints, onlySource } from "./actions.ts";
import type { ShowChoices, ShowSource } from "./choices.ts";
import { GroupChip, Only, Part } from "./controls.tsx";

interface Props {
  choices: ShowChoices;
  hidden: Hidden;
  onApply: Apply;
}

export function Points({ choices, hidden, onApply }: Props) {
  return (
    <Part title="Points" onAll={() => onApply(allPoints(choices))} onNone={() => onApply(nonePoints(choices))}>
      <Note>By kind of value</Note>
      <div className="grid grid-cols-2 gap-1" data-testid="show-kinds">
        {KINDS.map((kind) => (
          <GroupChip key={kind} group="kind" name={kind} label={kind} hidden={hidden} onApply={onApply}>
            {(on) => (
              <>
                <Mark kind={kind} dim={!on} />
                {kind}
              </>
            )}
          </GroupChip>
        ))}
      </div>
      <Sources choices={choices} hidden={hidden} onApply={onApply} />
      <Note>A catalogue is switched in every domain it is in; a domain's box switches all its points.</Note>
    </Part>
  );
}

/** A domain's number of points, short: in thousands from a thousand, in millions from what
 * would round to a thousand thousands. */
export const pointsText = (n: number): string => (n < 1e3 ? String(n) : n < 999_500 ? `${figures(n / 1e3)}k` : `${figures(n / 1e6)}M`);

/** A domain's name in its narrow column's head; its full name labels the column's switch. */
const COLUMN_NAMES: Readonly<Record<string, string>> = { stars: "Stars", nearby_galaxies: "Nearby", cosmic_web: "Web" };
const columnName = (name: string) => COLUMN_NAMES[name] ?? sentence(words(name));

/** The sources in one table, a column for each domain headed by its switch and its points. */
function Sources({ choices, hidden, onApply }: Props) {
  const groups = new Map<string | undefined, ShowSource[]>();
  for (const s of choices.sources) {
    const group = groups.get(s.domains[0]);
    if (group === undefined) groups.set(s.domains[0], [s]);
    else group.push(s);
  }
  return (
    <table className="w-full table-fixed border-collapse" data-testid="show-sources">
      <colgroup>
        <col />
        {choices.domains.map((d) => (
          <col key={d.name} className="w-9" />
        ))}
      </colgroup>
      <thead>
        <tr className="align-bottom text-xs text-dim">
          <th className="text-left font-normal">Catalogue</th>
          {choices.domains.map((d) => (
            <th key={d.name} className="font-normal" title={`${sentence(words(d.name))}: ${d.points.toLocaleString("en-GB")} points`}>
              <label className="flex flex-col items-center">
                <input
                  type="checkbox"
                  aria-label={sentence(words(d.name))}
                  checked={!hidden.domain.has(d.name)}
                  onChange={(e) => onApply([{ group: "domain", name: d.name, shown: e.currentTarget.checked }])}
                />
                <span className="text-ink">{columnName(d.name)}</span>
                <span>{pointsText(d.points)}</span>
              </label>
            </th>
          ))}
        </tr>
      </thead>
      {[...groups.values()].map((sources) => (
        <tbody key={sources[0]?.domains[0]} className="border-t border-line">
          {sources.map((s) => (
            <SourceRow key={s.id} source={s} domains={choices.domainNames} on={!hidden.source.has(s.id)} onApply={onApply} onOnly={() => onApply(onlySource(s.id, choices))} />
          ))}
        </tbody>
      ))}
    </table>
  );
}

/** A source on one line by its short name, its title on hover, marked in each domain it is in. */
function SourceRow({ source, domains, on, onApply, onOnly }: { source: ShowSource; domains: readonly string[]; on: boolean; onApply: Apply; onOnly: () => void }) {
  return (
    <tr className="group h-[22px]">
      <td className="relative">
        <label className="flex min-w-0 items-center gap-2" title={source.title}>
          <input type="checkbox" checked={on} onChange={(e) => onApply([{ group: "source", name: source.id, shown: e.currentTarget.checked }])} />
          <span className="truncate">{source.short}</span>
        </label>
        <Only what={source.short} onOnly={onOnly} place="over" />
      </td>
      {domains.map((d) => (
        <td key={d} className="text-center text-[9px]" style={{ color: POINT_CSS }}>
          {source.domains.includes(d) ? (
            <span role="img" aria-label={`in ${words(d)}`}>
              ●
            </span>
          ) : null}
        </td>
      ))}
    </tr>
  );
}
