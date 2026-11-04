import type { Geometry, StructureDetail, StructureSummary } from "../api/structures.ts";
import { words } from "./text.ts";

/** Every structure of the release by name and kind, each to choose. */
export function StructureList({ list, onChoose }: { list: StructureSummary[]; onChoose: (id: string) => void }) {
  return (
    <details className="rounded-lg border border-line bg-panel p-3 text-sm">
      <summary className="cursor-pointer">Structures ({list.length})</summary>
      <ul className="mt-2 max-h-64 overflow-y-auto" data-testid="structures">
        {list.map((s) => (
          <li key={s.id}>
            <button type="button" onClick={() => onChoose(s.id)} className="text-left hover:text-accent">
              {s.name} <span className="text-dim">({words(s.kind)})</span>
            </button>
          </li>
        ))}
      </ul>
    </details>
  );
}

/** A geometry's kind in words: a primitive by its form. */
const shapeKind = (geometry: Geometry): string => words(geometry.kind === "primitive" ? geometry.primitive.form : geometry.kind);

/** How a structure's shape was placed, in words. */
export function matchedText(detail: StructureDetail, approximate: boolean): string {
  if (detail.shape === null && detail.bounds === null) return "not placed: no shape and no bounding box, so it is listed but not drawn";
  const from = detail.derivation?.from?.map((p) => words(p.replace(/^\//, "").replaceAll("/", " "))).join("; ");
  const shape = detail.shape === null ? "no placed shape" : `a ${shapeKind(detail.shape.geometry)} in ${detail.shape.frame}`;
  const drawn = approximate ? ", drawn as its bounding box (approximate)" : "";
  return `${shape}${from === undefined || from === "" ? "" : `, derived from ${from}`}${drawn}`;
}

/** The chosen structure's record: kind, domain, how it was placed, and its cited values. */
export function StructurePanel({ detail, approximate, onClose }: { detail: StructureDetail; approximate: boolean; onClose: () => void }) {
  const papers = new Map((detail.record.papers ?? []).map((p) => [p.id, p.reference]));
  const values = Object.entries(detail.record.values ?? {});
  return (
    <section aria-label="The chosen structure" className="max-h-[50vh] space-y-2 overflow-y-auto rounded-lg border border-line bg-panel p-3 text-sm">
      <div className="flex items-start justify-between gap-2">
        <h2 className="font-semibold">{detail.name}</h2>
        <button type="button" onClick={onClose} className="text-dim hover:text-ink" aria-label="Close the structure's panel">
          ×
        </button>
      </div>
      <p data-testid="structure-kind">
        {words(detail.kind)} · {words(detail.scale_domain)}
      </p>
      <p className="text-xs text-dim" data-testid="structure-matched">
        Placed by {matchedText(detail, approximate)}.
      </p>
      <ul className="space-y-1" data-testid="structure-values">
        {values.map(([name, value]) => (
          <li key={name}>
            {words(name)}: {String(value.quantity?.value ?? "—")} {words(value.quantity?.unit ?? "")}
            <span className="text-dim">
              {" "}({value.kind ?? "kind not given"}; {value.cite === null || value.cite === undefined ? "no citation" : (papers.get(value.cite.paper) ?? value.cite.paper)})
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
