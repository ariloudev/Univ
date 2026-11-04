import type { StructureDetail } from "../api/structures.ts";
import { ObjectPanel } from "./ObjectPanel.tsx";
import { Panels } from "./Panels.tsx";
import { SearchBox } from "./SearchBox.tsx";
import { StructureList, StructurePanel } from "./StructuresPanel.tsx";
import type { ChooseOptions, ChosenObject } from "./useChosen.ts";
import type { Structures } from "./useStructures.ts";
import type { World } from "./useWorld.ts";

/** What is chosen, an object or a structure, and the ways to choose. */
export interface Selection {
  chosen: ChosenObject;
  structure: StructureDetail | undefined;
  /** Whether the chosen structure is drawn by its bounding box. */
  approximate: boolean;
  object: (id: number, options?: ChooseOptions) => void;
  structureById: (id: string) => void;
  close: () => void;
}

interface SidePanelsProps {
  world: World;
  release: number;
  cosmology: string;
  structures: Structures;
  selection: Selection;
}

/** The panels beside the view: where the eye is and the structures on the left; search and the
 * chosen object or structure on the right. */
export function SidePanels({ world, release, cosmology, structures, selection }: SidePanelsProps) {
  return (
    <>
      <div className="absolute top-3 left-3 flex flex-col gap-2">
        <Panels controller={world.controller} catalogue={() => world.catalogue} frame={world.frame} release={release} cosmology={cosmology} />
        <StructureList list={structures.list} onChoose={selection.structureById} />
        {structures.problem === undefined ? null : <p className="w-80 text-xs text-warn">{structures.problem}</p>}
      </div>
      <div className="absolute top-3 right-3 flex w-96 flex-col gap-2">
        <SearchBox release={release} onFound={(id, record, as) => selection.object(id, { record, as, fly: true })} />
        <ObjectPanel chosen={selection.chosen} onChoose={(id) => selection.object(id, { fly: true })} onClose={selection.close} />
        {selection.structure === undefined ? null : (
          <StructurePanel detail={selection.structure} approximate={selection.approximate} onClose={selection.close} />
        )}
      </div>
    </>
  );
}
