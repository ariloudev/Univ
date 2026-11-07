import { type ReactNode, useState } from "react";

import type { StructureDetail } from "../api/structures.ts";
import { AtlasFrame } from "./AtlasFrame.tsx";
import { ObjectPanel } from "./ObjectPanel.tsx";
import { KeyBar, ViewDrawer, Whereabouts } from "./Panels.tsx";
import { type DrawerName, Rail } from "./Rail.tsx";
import { ScaleBar } from "./ScaleBar.tsx";
import { SearchBox } from "./SearchBox.tsx";
import { ShowPanel, type ShowPanelProps } from "./ShowPanel.tsx";
import { type Inside, StructureList, StructurePanel } from "./StructuresPanel.tsx";
import type { ChooseOptions, ChosenObject } from "./useChosen.ts";
import type { ShownStructures } from "./useStructures.ts";
import type { World } from "./useWorld.ts";

/** What is chosen, an object or a structure, and the ways to choose. */
export interface Selection {
  chosen: ChosenObject;
  structure: StructureDetail | undefined;
  /** Whether the chosen structure is drawn by its bounding box. */
  approximate: boolean;
  /** Whether the view is culled to the chosen structure, and the way to turn it on or off;
   * none where the structure has no volume to cull to. */
  inside: Inside | undefined;
  object: (id: number, options?: ChooseOptions) => void;
  structureById: (id: string) => void;
  close: () => void;
}

interface AtlasProps {
  /** The view's world, once made. */
  world: World | undefined;
  release: number;
  cosmology: string;
  /** What the top bar holds before the view's own parts, and after the search. */
  bar: { start: ReactNode; end: ReactNode };
  structures: ShownStructures;
  selection: Selection;
  /** What can be turned off, what is, and the way to turn it on or off. */
  show: ShowPanelProps;
  /** The view: its canvas and what is drawn over it. */
  children: ReactNode;
}

const DRAWER_LABELS: Record<DrawerName, string> = { view: "The view", show: "Show", structures: "Structures" };

/** What is chosen, as one key: the record folded for one thing chosen opens again for the next. */
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

/** The explorer laid out as an atlas lays out a chart (frame-the-view-as-an-atlas): the top bar;
 * the rail and its drawers on the left; the view inside its neat line; the record on the right
 * while something is chosen; the bottom bar of scale, key and credits. Below 1,024 pixels a
 * drawer and the record open over the view. */
export function Atlas({ world, release, cosmology, bar, structures, selection, show, children }: AtlasProps) {
  const [drawer, setDrawer] = useState<DrawerName>();
  const [foldedFor, setFoldedFor] = useState<string>();
  const key = chosenKey(selection);
  return (
    <div className="grid h-full grid-cols-[minmax(0,1fr)] grid-rows-[40px_minmax(0,1fr)_30px] overflow-hidden">
      <TopBar world={world} release={release} bar={bar} selection={selection} />
      <div className="relative flex min-h-0">
        <Rail
          open={drawer}
          onToggle={(name) => {
            setDrawer(drawer === name ? undefined : name);
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
          {world === undefined ? null : <AtlasFrame world={world} />}
        </section>
        {key === undefined ? null : (
          <Record
            folded={foldedFor === key}
            onFold={(fold) => {
              setFoldedFor(fold ? key : undefined);
            }}
          >
            <ObjectPanel chosen={selection.chosen} onChoose={(id) => selection.object(id, { fly: true })} onClose={selection.close} />
            {selection.structure === undefined ? null : (
              <StructurePanel detail={selection.structure} approximate={selection.approximate} inside={selection.inside} onClose={selection.close} />
            )}
          </Record>
        )}
      </div>
      <footer className="flex min-w-0 items-center gap-5 border-t border-line bg-panel px-3 text-sm whitespace-nowrap">
        {world === undefined ? null : (
          <>
            <ScaleBar world={world} />
            <KeyBar catalogue={() => world.catalogue} frame={world.frame} release={release} />
          </>
        )}
      </footer>
    </div>
  );
}

/** The top bar: `bar` about the eye's distance with the axes and the search. */
function TopBar({ world, release, bar, selection }: Pick<AtlasProps, "world" | "release" | "bar" | "selection">) {
  return (
    <header className="flex min-w-0 items-center gap-4 border-b border-line bg-panel px-3 text-sm whitespace-nowrap">
      {bar.start}
      {world === undefined ? null : <Whereabouts controller={world.controller} frame={world.frame} />}
      <div className="ml-auto flex items-center gap-4">
        {world === undefined ? null : <SearchBox release={release} onFound={(id, record, as) => selection.object(id, { record, as, fly: true })} />}
        {bar.end}
      </div>
    </header>
  );
}

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
      aria-label={DRAWER_LABELS[name]}
      className="w-[300px] shrink-0 overflow-y-auto border-r border-line bg-panel p-3 text-sm max-lg:absolute max-lg:inset-y-0 max-lg:left-[52px] max-lg:z-20 max-lg:shadow-2xl"
    >
      {name === "view" ? <ViewDrawer controller={world.controller} frame={world.frame} cosmology={cosmology} /> : null}
      {name === "show" ? <ShowPanel {...show} /> : null}
      {name === "structures" ? (
        <>
          <StructureList list={structures.shown} onChoose={selection.structureById} />
          {structures.problem === undefined ? null : <p className="mt-2 text-xs text-warn">{structures.problem}</p>}
        </>
      ) : null}
    </aside>
  );
}

const OVER_THE_VIEW = "max-lg:absolute max-lg:inset-y-0 max-lg:right-0 max-lg:z-20";

/** The right margin holding the record of what is chosen, which folds to the window's edge. */
function Record({ folded, onFold, children }: { folded: boolean; onFold: (fold: boolean) => void; children: ReactNode }) {
  if (folded)
    return (
      <button
        type="button"
        onClick={() => {
          onFold(false);
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
            onFold(true);
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
