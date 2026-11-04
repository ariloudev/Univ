import { ObjectPanel } from "./ObjectPanel.tsx";
import { Panels } from "./Panels.tsx";
import { SearchBox } from "./SearchBox.tsx";
import { StructureList, StructurePanel } from "./StructuresPanel.tsx";
import type { ChosenObject } from "./useChosen.ts";
import type { Structures } from "./useStructures.ts";
import type { World } from "./useWorld.ts";

interface SidePanelsProps {
  world: World;
  release: number;
  cosmology: string;
  structures: Structures;
  chosen: ChosenObject;
  structureId: string | undefined;
  onChooseObject: (id: number) => void;
  onCloseObject: () => void;
  onChooseStructure: (id: string) => void;
  onCloseStructure: () => void;
}

/** The panels beside the view: where the eye is and the structures on the left; search and the
 * chosen object or structure on the right. */
export function SidePanels(props: SidePanelsProps) {
  const { world, structures, structureId } = props;
  const detail = structureId === undefined ? undefined : structures.details.get(structureId);
  return (
    <>
      <div className="absolute top-3 left-3 flex flex-col gap-2">
        <Panels controller={world.controller} catalogue={world.catalogue} release={props.release} cosmology={props.cosmology} />
        <StructureList list={structures.list} onChoose={props.onChooseStructure} />
        {structures.problem === undefined ? null : <p className="w-80 text-xs text-warn">{structures.problem}</p>}
      </div>
      <div className="absolute top-3 right-3 flex w-96 flex-col gap-2">
        <SearchBox release={props.release} onFound={props.onChooseObject} />
        <ObjectPanel chosen={props.chosen} onClose={props.onCloseObject} />
        {detail === undefined ? null : (
          <StructurePanel detail={detail} approximate={structures.drawings.get(detail.id)?.approximate ?? false} onClose={props.onCloseStructure} />
        )}
      </div>
    </>
  );
}
