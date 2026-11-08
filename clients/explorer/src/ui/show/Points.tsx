// The Show drawer's points (choose-what-is-drawn): the kinds of value as toggles with the key's
// marks, and each scale domain heading the sources of its points.

import { useEffect, useRef, useState } from "react";

import { KIND_MARKS, KINDS, POINT_CSS } from "../../scene/kinds.ts";
import type { Apply, Hidden } from "../hidden.ts";
import { counted, sentence } from "../record/wording.ts";
import { words } from "../text.ts";
import { allPoints, domainOff, domainOn, domainState, nonePoints, onlySource } from "./actions.ts";
import type { ShowChoices, ShowDomain, ShowSource } from "./choices.ts";
import { Caption, Chip, Only, PartHeading } from "./controls.tsx";

interface Props {
  choices: ShowChoices;
  hidden: Hidden;
  onApply: Apply;
}

export function Points({ choices, hidden, onApply }: Props) {
  return (
    <section className="space-y-1.5">
      <PartHeading title="Points" onAll={() => onApply(allPoints(choices))} onNone={() => onApply(nonePoints(choices))} />
      <Caption>By kind of value</Caption>
      <div className="grid grid-cols-2 gap-1" data-testid="show-kinds">
        {KINDS.map((kind) => {
          const on = !hidden.kind.has(kind);
          return (
            <Chip key={kind} on={on} name={kind} onPress={() => onApply([{ group: "kind", name: kind, shown: !on }])}>
              <span style={{ color: on ? POINT_CSS : undefined }}>{KIND_MARKS[kind]}</span>
              {kind}
            </Chip>
          );
        })}
      </div>
      <Caption>By domain and catalogue</Caption>
      {choices.domains.map((d) => (
        <Domain key={d.name} domain={d} choices={choices} hidden={hidden} onApply={onApply} />
      ))}
    </section>
  );
}

/** A domain's number of points: in full below a million, else in millions. */
const pointsText = (n: number): string => (n < 1e6 ? n.toLocaleString("en-GB") : `${(n / 1e6).toFixed(1)} million`);

/** A domain's heading, with its switch, its counts and its fold, and its sources beneath. */
function Domain({ domain, choices, hidden, onApply }: Props & { domain: ShowDomain }) {
  const [open, setOpen] = useState(true);
  const state = domainState(domain, hidden);
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (box.current !== null) box.current.indeterminate = state === "mixed";
  }, [state]);
  const name = sentence(words(domain.name));
  const shared = [...new Set(domain.sources.flatMap((s) => s.alsoIn))];
  return (
    <div className="border-t border-line pt-1" data-testid={`show-domain-${domain.name}`}>
      <div className="flex items-center gap-2">
        <label className="flex flex-1 items-center gap-2 font-semibold">
          <input ref={box} type="checkbox" checked={state === "on"} onChange={() => onApply(state === "on" ? domainOff(domain) : domainOn(domain))} />
          {name}
        </label>
        <span className="text-xs text-dim">
          {pointsText(domain.points)} · {counted(domain.sources.length, "catalogue")}
        </span>
        <button type="button" aria-expanded={open} aria-label={`${name}'s catalogues`} onClick={() => setOpen(!open)} className="w-3 text-[10px] text-dim">
          {open ? "▾" : "▸"}
        </button>
      </div>
      {open ? (
        <ul className="mb-1 ml-5">
          {domain.sources.map((s) => (
            <SourceRow key={s.id} source={s} on={!hidden.source.has(s.id)} onApply={onApply} onOnly={() => onApply(onlySource(s.id, choices))} />
          ))}
          {shared.length === 0 ? null : <li className="mt-0.5 text-xs text-dim">↔ also in {shared.map(words).join(" and ")}, and switched there too.</li>}
        </ul>
      ) : null}
    </div>
  );
}

/** A source on one line by its short name, its title on hover, marked where it is in other
 * domains too. */
function SourceRow({ source, on, onApply, onOnly }: { source: ShowSource; on: boolean; onApply: Apply; onOnly: () => void }) {
  return (
    <li className="group flex h-[22px] items-center gap-2">
      <label className="flex min-w-0 flex-1 items-center gap-2" title={source.title}>
        <input type="checkbox" checked={on} onChange={(e) => onApply([{ group: "source", name: source.id, shown: e.currentTarget.checked }])} />
        <span className="truncate">{source.short}</span>
      </label>
      <Only what={source.short} onOnly={onOnly} />
      {source.alsoIn.length === 0 ? null : (
        <span role="img" aria-label={`Also in ${source.alsoIn.map(words).join(" and ")}`} title={`Also in ${source.alsoIn.map(words).join(" and ")}, switched there too`} className="text-neat">
          ↔
        </span>
      )}
    </li>
  );
}
