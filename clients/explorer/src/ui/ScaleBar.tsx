import { unitsPerPixel } from "../scene/camera.ts";
import { scaleBar } from "../scene/scale.ts";
import { useStore } from "./store.ts";
import type { World } from "./useWorld.ts";

/** The scale of the view at the distance of its focus (frame-the-view-as-an-atlas). */
export function ScaleBar({ world }: { world: World }) {
  useStore(world.frame);
  const { controller } = world;
  const { height } = controller.size;
  if (height <= 0) return null;
  const bar = scaleBar(unitsPerPixel(controller.view, height));
  if (bar === undefined) return null;
  const w = bar.pixels;
  return (
    <div data-testid="scale" className="flex shrink-0 items-center gap-2" title="The scale of the view at the distance of its focus">
      <svg width={w + 2} height={10} aria-hidden="true" data-pixels={w.toFixed(1)}>
        <path d={`M1 1 V9 H${w + 1} V1 M${w / 2 + 1} 5 V9`} fill="none" stroke="var(--color-ink)" strokeWidth={1.25} />
      </svg>
      <span>
        <span className="sr-only">Scale at the focus: </span>
        {bar.label}
      </span>
    </div>
  );
}
