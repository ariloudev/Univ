import type { Geometry, StructureDetail, StructureSummary } from "../api/structures.ts";
import { structureName } from "../scene/structures.ts";
import { valueText, words } from "./text.ts";

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

/** A structure's published quantity with its uncertainty, as valueText words an object's. */
function quantityText(quantity: { value: unknown; unit?: string | undefined; uncertainty?: unknown } | undefined): string {
  if (quantity === undefined) return "not given";
  const { value, unit } = quantity;
  if (typeof value !== "number") return `${JSON.stringify(value)} ${words(unit ?? "")}`.trim();
  const uncertainty = quantity.uncertainty as Parameters<typeof valueText>[0]["uncertainty"];
  return valueText({ value, unit: unit ?? "", ...(uncertainty === undefined ? {} : { uncertainty }) });
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

/** Whether the view is culled to the chosen structure's volume, whether that volume is a box
 * rather than its published shape, and the way to turn it on or off. */
export interface Inside {
  on: boolean;
  approximate: boolean;
  set: (on: boolean) => void;
}

/** The checkbox that culls the view to the structure's volume, and what the cull is when on. */
function InsideCheckbox({ inside }: { inside: Inside }) {
  return (
    <div className="space-y-1">
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={inside.on} onChange={(e) => inside.set(e.target.checked)} />
        Only what lies inside
      </label>
      {inside.on ? (
        <p className="text-xs text-dim" data-testid="structure-inside">
          {inside.approximate ? "Approximate: the cull is to a box about the structure, not its published shape. " : ""}
          Points are placed at their preferred distance, so members scattered past its faces are not drawn.
        </p>
      ) : null}
    </div>
  );
}

/** The chosen structure's record: kind, domain, how it was placed, and its cited values; and,
 * where it has a volume, the checkbox that culls the view to it. */
export function StructurePanel({
  detail,
  approximate,
  inside,
  onClose,
}: {
  detail: StructureDetail;
  approximate: boolean;
  inside?: Inside | undefined;
  onClose: () => void;
}) {
  const papers = new Map((detail.record.papers ?? []).map((p) => [p.id, p.reference]));
  const values = Object.entries(detail.record.values ?? {});
  return (
    <section aria-label="The chosen structure" className="max-h-[50vh] space-y-2 overflow-y-auto rounded-lg border border-line bg-panel p-3 text-sm">
      <div className="flex items-start justify-between gap-2">
        <h2 className="font-semibold">{structureName(detail)}</h2>
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
      {detail.derivation?.extents === undefined ? null : (
        <p className="text-xs text-dim" data-testid="structure-extents">
          Its bounds are unknown: the lines are its published extents about its position, which need not be its middle.
        </p>
      )}
      {inside === undefined ? null : <InsideCheckbox inside={inside} />}
      <ul className="space-y-1" data-testid="structure-values">
        {values.map(([name, value]) => (
          <li key={name}>
            {words(name)}: {quantityText(value.quantity)}
            <span className="text-dim">
              {" "}({value.kind ?? "kind not given"}; {value.cite === null || value.cite === undefined ? "no citation" : (papers.get(value.cite.paper) ?? value.cite.paper)})
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
