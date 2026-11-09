import type { StructureSummary } from "../api/structures.ts";
import { words } from "./text.ts";

/** Every structure of the release by name and kind, each to choose. */
export function StructureList({ list, onChoose }: { list: StructureSummary[]; onChoose: (id: string) => void }) {
  return (
    <div>
      <h2 className="font-serif text-base">Structures ({list.length})</h2>
      <ul className="mt-2 space-y-0.5" data-testid="structures">
        {list.map((s) => (
          <li key={s.id}>
            <button type="button" onClick={() => onChoose(s.id)} className="text-left hover:text-accent">
              <span className="font-serif italic">{s.name}</span> <span className="text-dim">({words(s.kind)})</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
