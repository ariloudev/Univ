import { CHOSEN_CSS, STRUCTURE_CSS } from "../scene/kinds.ts";
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

/** Each placed structure's name, kept at its centre and clear of the others. The names take no
 * pointer: the view's mouse chooses a structure by its name (and the list does, by keyboard). */
export function StructureNames({ names, overlay, chosen }: { names: readonly NameLabel[]; overlay: Overlay; chosen?: string | undefined }) {
  return (
    <div ref={overlay.names} className="pointer-events-none">
      {names.map((name) => (
        <span
          key={name.id}
          data-structure={name.id}
          className={`absolute top-0 left-0 font-serif text-xs whitespace-nowrap italic ${name.id === chosen ? "font-medium" : ""}`}
          style={{ display: "none", color: name.id === chosen ? CHOSEN_CSS : STRUCTURE_CSS }}
        >
          <span className="block -translate-x-1/2 -translate-y-1/2">{name.text}</span>
        </span>
      ))}
    </div>
  );
}

/** The label of what is under the pointer: a structure's name, which a click reads, or a point. */
export function HoverLabel({ world }: { world: World }) {
  const hover = useStore(world.hover);
  if (hover === undefined) return null;
  return (
    <p role="tooltip" className="pointer-events-none absolute rounded bg-panel px-2 py-1 text-xs" style={{ left: hover.x + 12, top: hover.y + 12 }}>
      {"name" in hover ? (
        <>
          <span className="font-serif italic">{hover.name.text}</span>. Click to read the structure.
        </>
      ) : (
        pointText(hover.point)
      )}
    </p>
  );
}
