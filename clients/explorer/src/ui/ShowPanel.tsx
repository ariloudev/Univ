// The checkboxes that turn groups of the view on and off (choose-what-is-drawn): the scale
// domains, the sources of the drawn points, the kinds of value and the kinds of structure.

import type { ReactNode } from "react";

import { KINDS, cssColour } from "../scene/kinds.ts";
import { MASK_BITS } from "../scene/masks.ts";
import { Swatch } from "./Swatch.tsx";
import type { Group, Hidden, Toggle } from "./hidden.ts";
import { words } from "./text.ts";

/** What the panel offers to turn off: the release's domains, the sources of its points'
 * distances by id with their titles, and its kinds of structure with how many of each. */
export interface ShowChoices {
  domains: readonly string[];
  sources: readonly { id: string; title: string }[];
  structureKinds: readonly { kind: string; count: number }[];
}

export interface ShowPanelProps {
  choices: ShowChoices;
  hidden: Hidden;
  onToggle: Toggle;
}

interface BoxProps {
  group: Group;
  name: string;
  label: string;
  hidden: Hidden;
  onToggle: Toggle;
  swatch?: string;
}

function Box({ group, name, label, hidden, onToggle, swatch }: BoxProps) {
  return (
    <li>
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={!hidden[group].has(name)} onChange={(e) => onToggle(group, name, e.currentTarget.checked)} />
        {swatch === undefined ? null : <Swatch colour={swatch} />}
        {label}
      </label>
    </li>
  );
}

function Fieldset({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="mt-2">
      <legend className="text-xs text-dim">{legend}</legend>
      <ul className="space-y-0.5">{children}</ul>
    </fieldset>
  );
}

/** A panel of checkboxes, one for each group that can be turned off, checked while it is drawn. */
export function ShowPanel({ choices, hidden, onToggle }: ShowPanelProps) {
  const box = { hidden, onToggle };
  return (
    <details className="w-80 rounded-lg border border-line bg-panel p-3 text-sm">
      <summary className="cursor-pointer">Show</summary>
      <div data-testid="show" className="max-h-96 overflow-y-auto">
        <Fieldset legend="Scale domains">
          {choices.domains.map((d) => (
            <Box key={d} group="domain" name={d} label={words(d)} {...box} />
          ))}
        </Fieldset>
        <Fieldset legend="Catalogues: the source of each point's distance">
          {choices.sources.map((s) => (
            <Box key={s.id} group="source" name={s.id} label={s.title} {...box} />
          ))}
        </Fieldset>
        <Fieldset legend="Kinds of value">
          {KINDS.map((k) => (
            <Box key={k} group="kind" name={k} label={k} swatch={cssColour(k)} {...box} />
          ))}
        </Fieldset>
        <Fieldset legend="Kinds of structure">
          {choices.structureKinds.map(({ kind, count }) => (
            <Box key={kind} group="structure" name={kind} label={`${words(kind)} (${count})`} {...box} />
          ))}
        </Fieldset>
      </div>
    </details>
  );
}

interface ChoiceDomain {
  name: string;
  manifest: { sources: readonly { index: number; id: string; title: string; role: string }[] };
}

/** What `domains` and `structures` offer to turn off: every domain; every source of a point's
 * distance once, of an index a mask can hold (one that only places points on the sky decides
 * no point's distance, and turning it off would hide nothing); and every kind of structure with
 * its count; each in the order first met. */
export function choicesOf(domains: readonly ChoiceDomain[], structures: readonly { kind: string }[]): ShowChoices {
  const sources = new Map<string, string>();
  for (const d of domains) {
    for (const s of d.manifest.sources) if (s.role !== "position" && s.index < MASK_BITS && !sources.has(s.id)) sources.set(s.id, s.title);
  }
  const kinds = new Map<string, number>();
  for (const s of structures) kinds.set(s.kind, (kinds.get(s.kind) ?? 0) + 1);
  return {
    domains: domains.map((d) => d.name),
    sources: [...sources].map(([id, title]) => ({ id, title })),
    structureKinds: [...kinds].map(([kind, count]) => ({ kind, count })),
  };
}
