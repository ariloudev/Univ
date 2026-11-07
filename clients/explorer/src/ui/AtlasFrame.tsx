import { type EdgeTicks, edgeTicks } from "../scene/graticule.ts";
import { useStore } from "./store.ts";
import type { World } from "./useWorld.ts";

/** How far inside the view's edge the neat line runs, the ticks reach, and the edge's name
 * stands, CSS pixels. */
const NEAT = 2.5;
const TICK = 9;
const NAME = 34;
/** How far inside a side the side's name stands, clear of its labels. */
const SIDE_NAME = 48;
/** Ticks this near a corner are left out, clear of the other edge's labels. */
const CORNER = 28;

/** The view's neat line and the coordinates on its edges (frame-the-view-as-an-atlas), drawn just
 * inside it, redrawn with every frame of the view. */
export function AtlasFrame({ world }: { world: World }) {
  useStore(world.frame);
  const { controller } = world;
  const { width, height } = controller.size;
  if (width <= 0 || height <= 0) return null;
  const edges = edgeTicks(controller.view, controller.toFrame, controller.axes, width, height);
  return (
    <svg
      aria-hidden="true"
      data-testid="atlas-frame"
      className="pointer-events-none absolute inset-0 h-full w-full font-sans text-[11px]"
      viewBox={`0 0 ${width} ${height}`}
    >
      <rect x={NEAT} y={NEAT} width={width - 2 * NEAT} height={height - 2 * NEAT} fill="none" stroke="var(--color-neat)" strokeWidth={1.5} />
      {edges.map((edge) => (
        <EdgeMarks key={edge.edge} edge={edge} width={width} height={height} />
      ))}
    </svg>
  );
}

/** Text kept legible over the points by a rim of the view's dark. */
const halo = { stroke: "var(--color-space)", strokeWidth: 3, paintOrder: "stroke", strokeLinejoin: "round" } as const;

function EdgeMarks({ edge, width, height }: { edge: EdgeTicks; width: number; height: number }) {
  const along = edge.edge === "top" || edge.edge === "bottom";
  const length = along ? width : height;
  const ticks = edge.ticks.filter((t) => t.at >= CORNER && t.at <= length - CORNER);
  // Where a tick of this edge runs, and where its label and the edge's name stand.
  const place = (at: number, inset: number): [number, number] => {
    switch (edge.edge) {
      case "top":
        return [at, inset];
      case "bottom":
        return [at, height - inset];
      case "left":
        return [inset, at];
      case "right":
        return [width - inset, at];
    }
  };
  const anchor = along ? "middle" : edge.edge === "left" ? "start" : "end";
  const [nx, ny] = place(length / 2, along ? NAME : SIDE_NAME);
  const turn = edge.edge === "left" ? -90 : edge.edge === "right" ? 90 : 0;
  return (
    <g data-edge={edge.edge} data-coordinate={edge.name}>
      {ticks.map((tick) => {
        const [x1, y1] = place(tick.at, NEAT);
        const [x2, y2] = place(tick.at, TICK);
        const [lx, ly] = place(tick.at, along ? TICK + 4 : TICK + 3);
        return (
          <g key={`${tick.value}@${Math.round(tick.at)}`}>
            <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="var(--color-neat)" strokeWidth={1.25} />
            <text
              x={lx}
              y={ly}
              textAnchor={anchor}
              dominantBaseline={edge.edge === "top" ? "hanging" : edge.edge === "bottom" ? "auto" : "middle"}
              fill="var(--color-ink)"
              {...halo}
            >
              {tick.label}
            </text>
          </g>
        );
      })}
      <text
        x={nx}
        y={ny}
        textAnchor="middle"
        dominantBaseline={edge.edge === "bottom" ? "auto" : "hanging"}
        transform={turn === 0 ? undefined : `rotate(${turn} ${nx} ${ny})`}
        fill="var(--color-dim)"
        {...halo}
      >
        {edge.name}
      </text>
    </g>
  );
}
