import type { StructureDrawing } from "../scene/structures.ts";
import type { Overlay } from "./overlay.ts";
import type { ScenePoint } from "./scenePoint.ts";
import { pointText } from "./text.ts";

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
export function StructureNames({ drawings, overlay, onChoose }: { drawings: Iterable<StructureDrawing>; overlay: Overlay; onChoose: (id: string) => void }) {
  const named = [...drawings].flatMap((d) => (d.centre === undefined ? [] : [{ ...d, centre: d.centre }]));
  return named.map((d) => (
    <button
      key={d.id}
      type="button"
      ref={(element) => {
        if (element === null) overlay.labels.current.delete(d.id);
        else overlay.labels.current.set(d.id, { element, centre: d.centre });
      }}
      onClick={() => onChoose(d.id)}
      className="absolute top-0 left-0 text-xs whitespace-nowrap text-[#ff8c59] hover:underline"
      style={{ display: "none" }}
    >
      <span className="block -translate-x-1/2 -translate-y-1/2">
        {d.name}
        {d.approximate ? " (approximate)" : ""}
      </span>
    </button>
  ));
}

/** The label of the point under the pointer. */
export function HoverLabel({ hover }: { hover: { x: number; y: number; point: ScenePoint } | undefined }) {
  if (hover === undefined) return null;
  return (
    <p role="tooltip" className="pointer-events-none absolute rounded bg-panel px-2 py-1 text-xs" style={{ left: hover.x + 12, top: hover.y + 12 }}>
      {pointText(hover.point)}
    </p>
  );
}
