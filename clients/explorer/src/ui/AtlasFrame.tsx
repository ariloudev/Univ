import { useMemo } from "react";

import { type Edge, type EdgeTicks, across, edgePoint, edgeTicks } from "../scene/graticule.ts";
import { ORIGIN } from "../scene/vec.ts";
import { useStore } from "./store.ts";
import type { World } from "./useWorld.ts";

/** How far inside the view's edge the neat line runs, the ticks reach, and an edge's name stands
 * (farther on the sides, clear of their wider labels), CSS pixels. */
const NEAT = 2.5;
const TICK = 9;
const NAME = 34;
const SIDE_NAME = 48;
/** Ticks this near a corner are left out, clear of the other edge's labels. */
const CORNER = 28;

/** How each edge sets its labels and its name: the labels' anchor and baseline, how far inside
 * them, and the name's baseline and turn. */
const LAYOUT: Record<Edge, { anchor: "middle" | "start" | "end"; baseline: "hanging" | "auto" | "middle"; label: number; name: number; nameBaseline: "hanging" | "auto"; turn: number }> = {
  top: { anchor: "middle", baseline: "hanging", label: TICK + 4, name: NAME, nameBaseline: "hanging", turn: 0 },
  bottom: { anchor: "middle", baseline: "auto", label: TICK + 4, name: NAME, nameBaseline: "auto", turn: 0 },
  left: { anchor: "start", baseline: "middle", label: TICK + 3, name: SIDE_NAME, nameBaseline: "hanging", turn: -90 },
  right: { anchor: "end", baseline: "middle", label: TICK + 3, name: SIDE_NAME, nameBaseline: "hanging", turn: 90 },
};

/** The view's neat line and the coordinates on its edges (frame-the-view-as-an-atlas), drawn just
 * inside it. They are read again with every frame drawn, and worked out only when the view, its
 * axes or its size changed: a frame that only fades tiles in keeps them. */
export function AtlasFrame({ world }: { world: World }) {
  useStore(world.frame);
  const { controller } = world;
  const { toFrame, axes } = controller;
  // The ticks depend on the way the eye looks, not where it is: moving or zooming keeps them.
  const { yaw, pitch } = controller.view;
  const { width, height } = controller.size;
  return useMemo(() => {
    if (width <= 0 || height <= 0) return null;
    const edges = edgeTicks({ focus: ORIGIN, distance: 1, yaw, pitch }, toFrame, axes, width, height);
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
  }, [yaw, pitch, toFrame, axes, width, height]);
}

/** Text kept legible over the points by a rim of the view's dark. */
const halo = { stroke: "var(--color-space)", strokeWidth: 3, paintOrder: "stroke", strokeLinejoin: "round" } as const;

function EdgeMarks({ edge, width, height }: { edge: EdgeTicks; width: number; height: number }) {
  const layout = LAYOUT[edge.edge];
  const length = across(edge.edge) ? width : height;
  const at = (along: number, inset: number) => edgePoint(edge.edge, along, inset, width, height);
  const [nx, ny] = at(length / 2, layout.name);
  return (
    <g data-edge={edge.edge} data-coordinate={edge.name}>
      {edge.ticks
        .filter((t) => t.at >= CORNER && t.at <= length - CORNER)
        .map((tick) => {
          const [x1, y1] = at(tick.at, NEAT);
          const [x2, y2] = at(tick.at, TICK);
          const [lx, ly] = at(tick.at, layout.label);
          return (
            <g key={`${tick.value}@${Math.round(tick.at)}`}>
              <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="var(--color-neat)" strokeWidth={1.25} />
              <text x={lx} y={ly} textAnchor={layout.anchor} dominantBaseline={layout.baseline} fill="var(--color-ink)" {...halo}>
                {tick.label}
              </text>
            </g>
          );
        })}
      <text
        x={nx}
        y={ny}
        textAnchor="middle"
        dominantBaseline={layout.nameBaseline}
        transform={layout.turn === 0 ? undefined : `rotate(${layout.turn} ${nx} ${ny})`}
        fill="var(--color-dim)"
        {...halo}
      >
        {edge.name}
      </text>
    </g>
  );
}
