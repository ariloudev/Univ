// The whole record of a structure, as its panel's link opens it (see-the-structures): its notes,
// the claims that dispute it, how it was placed, each value with its paper and the words its paper
// prints it in, and every paper in full.

import type { StructureRecord } from "../../api/structures.ts";
import { valueText, words } from "../text.ts";
import { Note, Section } from "./parts.tsx";
import { matchedText } from "./structureText.ts";

/** A structure's published quantity with its uncertainty, as valueText words an object's. */
function quantityText(quantity: { value: unknown; unit?: string | undefined; uncertainty?: unknown } | undefined): string {
  if (quantity === undefined) return "not given";
  const { value, unit } = quantity;
  if (typeof value !== "number") return `${JSON.stringify(value)} ${words(unit ?? "")}`.trim();
  const uncertainty = quantity.uncertainty as Parameters<typeof valueText>[0]["uncertainty"];
  return valueText({ value, unit: unit ?? "", ...(uncertainty === undefined ? {} : { uncertainty }) });
}

export function FullStructureRecord({ s: detail, approximate }: { s: StructureRecord; approximate: boolean }) {
  const papers = new Map((detail.record.papers ?? []).map((p) => [p.id, p.reference]));
  const disputes = detail.record.disputes ?? [];
  return (
    <>
      {detail.record.notes === null || detail.record.notes === undefined ? null : (
        <Section title="Notes" testid="structure-notes">
          <p>{detail.record.notes}</p>
        </Section>
      )}
      {disputes.length === 0 ? null : (
        <Section title="Disputed" testid="structure-disputes">
          <ul className="space-y-1">
            {disputes.map((d, i) => (
              <li key={i}>
                {d.claim} <span className="text-dim">({papers.get(d.cite.paper) ?? d.cite.paper})</span>
                {d.cite.locator?.quote === undefined ? null : <Quote text={d.cite.locator.quote} />}
              </li>
            ))}
          </ul>
        </Section>
      )}
      <Section title="How it was placed" testid="structure-placed">
        <Note testid="structure-matched">Placed by {matchedText(detail, approximate)}.</Note>
      </Section>
      <Section title="Every value" testid="structure-values">
        <ul className="space-y-1.5">
          {Object.entries(detail.record.values ?? {}).map(([name, value]) => (
            <li key={name}>
              {words(name)}: {quantityText(value.quantity)}
              <span className="text-dim">
                {" "}({value.kind ?? "kind not given"}; {value.cite === null || value.cite === undefined ? "no citation" : (papers.get(value.cite.paper) ?? value.cite.paper)})
              </span>
              {value.cite?.locator?.quote === undefined ? null : <Quote text={value.cite.locator.quote} />}
            </li>
          ))}
        </ul>
      </Section>
      <Section title="Papers" testid="structure-papers">
        <ul className="space-y-0.5">
          {[...papers].map(([id, reference]) => (
            <li key={id}>{reference}</li>
          ))}
        </ul>
      </Section>
    </>
  );
}

/** The words a paper prints a value or claim in. */
const Quote = ({ text }: { text: string }) => <q className="mt-0.5 block border-l border-line pl-2 text-xs text-dim">{text}</q>;
