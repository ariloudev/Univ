import { type ReactNode, useMemo, useState } from "react";
import { createPortal } from "react-dom";

import type { Cosmology } from "../api/schemas.ts";
import type { StructureDetail } from "../api/structures.ts";
import { AtlasFrame } from "./AtlasFrame.tsx";
import type { Switch } from "./addressSwitch.ts";
import { useGrid } from "./gridSwitch.ts";
import type { Inside } from "./inside.ts";
import { useDrawnRings } from "./ringsSwitch.ts";
import { ObjectPanel, type RecordContext } from "./ObjectPanel.tsx";
import { KeyBar, ViewDrawer, Whereabouts } from "./Panels.tsx";
import { type DrawerName, Rail, drawerTitle } from "./Rail.tsx";
import { ScaleBar } from "./ScaleBar.tsx";
import { SearchBox } from "./SearchBox.tsx";
import { ShowPanel, type ShowPanelProps } from "./show/ShowPanel.tsx";
import { StructureRecordPanel } from "./record/StructureRecordPanel.tsx";
import { DepthIndex } from "./structures/DepthIndex.tsx";
import { depthRows } from "./structures/depth.ts";
import { SwitchBox } from "./SwitchBox.tsx";
import type { ChooseOptions, ChosenObject } from "./useChosen.ts";
import type { ShownStructures } from "./useStructures.ts";
import type { World } from "./useWorld.ts";

/** What is chosen, an object or a structure, and the ways to choose. */
export interface Selection {
  chosen: ChosenObject;
  structure: StructureDetail | undefined;
  /** The chosen structure's id, as soon as it is chosen, before its record is read. */
  structureId: string | undefined;
  /** Whether the chosen structure is drawn by its bounding box. */
  approximate: boolean;
  /** Whether the view is culled to the chosen structure, and the way to turn it on or off;
   * none where the structure has no volume to cull to. */
  inside: Inside | undefined;
  object: (id: number, options?: ChooseOptions) => void;
  /** What the object's panel reads besides its record. */
  context?: RecordContext;
  structureById: (id: string) => void;
  /** Chooses structure `id` from the list of structures, or lets it go where it is chosen. */
  structureFromList: (id: string) => void;
  /** Lets go of what is chosen, closing its record, and makes Earth the focus again. */
  close: () => void;
}

/** Where the view's parts of the page go, and the drawer open: kept by the page, so that they
 * outlive the view of one release. */
export interface AtlasPlace {
  /** The top bar's place for the eye's distance, the axes and the search. */
  topBar: HTMLElement | null;
  drawer: DrawerName | undefined;
  onDrawer: (drawer: DrawerName | undefined) => void;
}

interface AtlasProps {
  /** The view's world, once made. */
  world: World | undefined;
  release: number;
  /** The release cosmology: its name, and what places a structure's velocities. */
  cosmology: Cosmology | undefined;
  place: AtlasPlace;
  structures: ShownStructures;
  selection: Selection;
  /** What can be turned off, what is, and the way to turn it on or off. */
  show: ShowPanelProps;
  /** The structures' outlines' switch, which the structures drawn follow. */
  outlines: Switch;
  /** The view: its canvas and what is drawn over it. */
  children: ReactNode;
}

/** What is chosen, as one key: the record is made anew for each thing chosen, so one folded
 * opens again for the next. */
function chosenKey(selection: Selection): string | undefined {
  if (selection.structure !== undefined) return `structure:${selection.structure.id}`;
  const { chosen } = selection;
  switch (chosen.kind) {
    case "none":
      return undefined;
    case "ready":
      return `object:${chosen.record.id}`;
    case "choices":
      return "choices";
    default:
      return `object:${chosen.id}`;
  }
}

/** The page laid out as an atlas lays out a chart (frame-the-view-as-an-atlas): the top bar of
 * `start`, the view's own parts placed into `slot`, and `end`; then `children`, the view's rows.
 * It outlives the view of one release, so the release's picker keeps its focus. */
export function AtlasPage({ start, end, slot, children }: { start: ReactNode; end: ReactNode; slot: (element: HTMLElement | null) => void; children: ReactNode }) {
  return (
    <div className="grid h-full grid-cols-[minmax(0,1fr)] grid-rows-[40px_minmax(0,1fr)_30px] overflow-hidden">
      <header className="flex min-w-0 items-center gap-4 border-b border-line bg-panel px-3 text-sm whitespace-nowrap">
        {start}
        <div ref={slot} className="flex min-w-0 flex-1 items-center gap-4" />
        {end}
      </header>
      {children}
    </div>
  );
}

/** The view's rows of the page: the rail and its drawers on the left; the view inside its neat
 * line; the record on the right while something is chosen; the bottom bar of scale, key and
 * credits; and, in the top bar, the eye's distance, the axes and the search. Below 1,024 pixels a
 * drawer and the record open over the view. */
export function Atlas({ world, release, cosmology, place, structures, selection, show, outlines, children }: AtlasProps) {
  const { drawer, onDrawer } = place;
  const key = chosenKey(selection);
  const grid = useGrid();
  const rings = useDrawnRings(world?.controller);
  return (
    <>
      {place.topBar === null || world === undefined ? null : createPortal(<TopBarParts world={world} release={release} selection={selection} switches={{ grid, rings, outlines }} />, place.topBar)}
      <div className="relative flex min-h-0">
        <Rail
          open={drawer}
          onToggle={(name) => {
            onDrawer(drawer === name ? undefined : name);
          }}
          onEarth={() => {
            world?.controller.returnToEarth();
          }}
        />
        {drawer === undefined || world === undefined ? null : (
          <Drawer name={drawer} world={world} cosmology={cosmology} structures={structures} selection={selection} show={show} />
        )}
        <section aria-label="The view" data-release={release} className="relative min-w-0 flex-1">
          {children}
          {world === undefined ? null : <AtlasFrame world={world} grid={grid[0]} />}
        </section>
        {key === undefined ? null : <Chosen key={key} selection={selection} cosmology={cosmology} />}
      </div>
      <footer className="flex min-w-0 items-center gap-5 border-t border-line bg-panel px-3 text-sm whitespace-nowrap">
        {world === undefined ? null : (
          <>
            <ScaleBar world={world} />
            <KeyBar catalogue={() => world.catalogue} frame={world.frame} release={release} />
          </>
        )}
        {structures.problem === undefined ? null : (
          <p role="status" className="ml-auto shrink-0 text-warn" title={structures.problem}>
            Structures not read
          </p>
        )}
      </footer>
    </>
  );
}

/** The record of what `selection` holds chosen, in the right margin. */
function Chosen({ selection, cosmology }: { selection: Selection; cosmology: Cosmology | undefined }) {
  return (
    <Record>
      <ObjectPanel chosen={selection.chosen} onChoose={(id) => selection.object(id, { fly: true })} onClose={selection.close} context={selection.context} />
      {selection.structure === undefined ? null : (
        <StructureRecordPanel detail={selection.structure} approximate={selection.approximate} inside={selection.inside} cosmology={cosmology} onClose={selection.close} />
      )}
    </Record>
  );
}

/** The view's parts of the top bar: the eye's distance with the axes and the switches of the grid,
 * the distance rings and the structures' outlines, the search, and, while a structure drawn as a
 * volume is chosen, the cull to it beside the level-of-detail switch that follows. */
function TopBarParts({ world, release, selection, switches }: { world: World; release: number; selection: Selection; switches: Record<"grid" | "rings" | "outlines", Switch> }) {
  return (
    <>
      <Whereabouts controller={world.controller} frame={world.frame} {...switches} />
      <div className="ml-auto flex items-center gap-4">
        <SearchBox release={release} onFound={(id, record, as) => selection.object(id, { record, as, fly: true })} />
        {selection.inside === undefined ? null : <InsideBox inside={selection.inside} />}
      </div>
    </>
  );
}

/** The checkbox that culls the view to the chosen structure's volume: its words shown from 1,536
 * pixels, where the top bar has room for them beside the rest. */
const InsideBox = ({ inside }: { inside: Inside }) => <SwitchBox label="Only what lies inside" state={[inside.on, inside.set]} textClass="max-2xl:sr-only" role="checkbox" />;

/** The drawer `name` the rail opened, beside the view, or over it below 1,024 pixels. */
function Drawer({
  name,
  world,
  cosmology,
  structures,
  selection,
  show,
}: { name: DrawerName; world: World } & Pick<AtlasProps, "cosmology" | "structures" | "selection" | "show">) {
  return (
    <aside
      id={`drawer-${name}`}
      aria-label={drawerTitle(name)}
      className="w-[300px] shrink-0 overflow-y-auto border-r border-line bg-panel p-3 text-sm max-lg:absolute max-lg:inset-y-0 max-lg:left-[52px] max-lg:z-20 max-lg:shadow-2xl"
    >
      {name === "view" ? <ViewDrawer controller={world.controller} frame={world.frame} cosmology={cosmology?.name ?? ""} /> : null}
      {name === "show" ? <ShowPanel {...show} /> : null}
      {name === "structures" ? (
        <>
          <StructuresDrawer structures={structures} selection={selection} />
          {structures.problem === undefined ? null : <p className="mt-2 text-xs text-warn">{structures.problem}</p>}
        </>
      ) : null}
    </aside>
  );
}

/** The structures drawer: the depth index of the structures drawn, the chosen one marked. */
function StructuresDrawer({ structures, selection }: { structures: ShownStructures; selection: Selection }) {
  const rows = useMemo(() => depthRows(structures.shown, structures), [structures]);
  return <DepthIndex rows={rows} chosen={selection.structureId} onChoose={selection.structureFromList} />;
}

const OVER_THE_VIEW = "max-lg:absolute max-lg:inset-y-0 max-lg:right-0 max-lg:z-20";

/** The right margin holding the record of what is chosen, which folds to the window's edge. */
function Record({ children }: { children: ReactNode }) {
  const [folded, setFolded] = useState(false);
  if (folded)
    return (
      <button
        type="button"
        onClick={() => {
          setFolded(false);
        }}
        className={`w-7 shrink-0 border-l border-line bg-panel py-3 text-dim hover:text-accent ${OVER_THE_VIEW}`}
      >
        <span className="[writing-mode:vertical-rl]">Open the record</span>
      </button>
    );
  return (
    <aside aria-label="The record" className={`flex w-[360px] shrink-0 flex-col border-l border-line bg-panel max-lg:w-[min(360px,calc(100%-52px))] max-lg:shadow-2xl ${OVER_THE_VIEW}`}>
      <div className="flex justify-end border-b border-line px-2 py-1 text-sm">
        <button
          type="button"
          onClick={() => {
            setFolded(true);
          }}
          className="rounded px-1.5 text-dim hover:text-accent"
        >
          Fold the record
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
    </aside>
  );
}
