import { STRUCTURE_CSS } from "../scene/kinds.ts";
import type { NameLabel, Overlay } from "./overlay.ts";
import { useStore } from "./store.ts";
import { pointText } from "./text.ts";
import type { World } from "./useWorld.ts";

/** Earth, the ring's label and the chosen object's mark, kept over their places. */
export function Marks({ overlay }: { overlay: Overlay }) {
  return (
    <>
      <div ref={overlay.earth} className="pointer-events-none absolute top-0 left-0" title="The Solar System barycentre, within about 1 AU of Earth">
        <div className="-translate-x-1/2 -translate-y-1/2 text-xs text-accent">
          <span className="mx-auto block h-2 w-2 rounded-full bg-accent" />
          <span className="mt-1 block whitespace-nowrap">Earth</span>
        </div>
      </div>
      <div ref={overlay.ring} className="pointer-events-none absolute top-0 left-0 text-xs text-dim">
        <span ref={overlay.ringLabel} className="ml-1 whitespace-nowrap" />
      </div>
      <div ref={overlay.chosen} className="pointer-events-none absolute top-0 left-0" data-testid="chosen-mark" style={{ display: "none" }}>
        <span className="block h-5 w-5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-warn" />
      </div>
    </>
  );
}

/** Each placed structure's name, kept at its centre and clear of the others; each to choose. */
export function StructureNames({ names, overlay, onChoose }: { names: readonly NameLabel[]; overlay: Overlay; onChoose: (id: string) => void }) {
  return (
    <div ref={overlay.names}>
      {names.map((name) => (
        <button
          key={name.id}
          type="button"
          data-structure={name.id}
          onClick={() => onChoose(name.id)}
          className="absolute top-0 left-0 text-xs whitespace-nowrap hover:underline"
          style={{ display: "none", color: STRUCTURE_CSS }}
        >
          <span className="block -translate-x-1/2 -translate-y-1/2">{name.text}</span>
        </button>
      ))}
    </div>
  );
}

/** The label of the point under the pointer. */
export function HoverLabel({ world }: { world: World }) {
  const hover = useStore(world.hover);
  if (hover === undefined) return null;
  return (
    <p role="tooltip" className="pointer-events-none absolute rounded bg-panel px-2 py-1 text-xs" style={{ left: hover.x + 12, top: hover.y + 12 }}>
      {pointText(hover.point)}
    </p>
  );
}
