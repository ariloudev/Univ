import { useEffect, useState } from "react";

import { recordOf } from "../api/objects.ts";
import { CHOSEN_CSS, STRUCTURE_CSS } from "../scene/kinds.ts";
import type { NameLabel, Overlay } from "./overlay.ts";
import { useStore } from "./store.ts";
import { headingOf, pointText } from "./text.ts";
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

/** How long the pointer rests on a point before its label names it, milliseconds. */
const REST_MS = 300;

/** The name or first designation of object `id` of `release`, read from its record once the
 * pointer has rested on it for `REST_MS`; none before, or for another object. */
function useRestedName(id: number | undefined, release: number): string | undefined {
  const [named, setNamed] = useState<{ id: number; name: string }>();
  useEffect(() => {
    if (id === undefined) return;
    let live = true;
    const timer = setTimeout(() => {
      recordOf(id, release)
        .then((found) => {
          if (live && found.kind === "object") setNamed({ id, name: headingOf(found.record) });
        })
        .catch(() => undefined);
    }, REST_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [id, release]);
  return named !== undefined && named.id === id ? named.name : undefined;
}

/** The label of what is under the pointer: a structure's name, which a click reads, or a point,
 * named from its record once the pointer has rested on it. */
export function HoverLabel({ world, release }: { world: World; release: number }) {
  const hover = useStore(world.hover);
  const point = hover !== undefined && "point" in hover ? hover.point : undefined;
  const name = useRestedName(point?.id, release);
  if (hover === undefined) return null;
  return (
    <p role="tooltip" className="pointer-events-none absolute rounded bg-panel px-2 py-1 text-xs" style={{ left: hover.x + 12, top: hover.y + 12 }}>
      {"name" in hover ? (
        <>
          <span className="font-serif italic">{hover.name.text}</span>. Click to read the structure.
        </>
      ) : (
        pointText(hover.point, name)
      )}
    </p>
  );
}
