import { useState } from "react";

import { words } from "../text.ts";
import { DEPTH_TICKS, type DepthRow, KNOWN_WORDS, type Known, banded, byName, depthX, distanceText, matching } from "./depth.ts";

/** The width of each row's axis, and of the ticks above them, CSS pixels. */
const AXIS = 168;
const TRACK = 12;

/** The mark of how a structure is known: filled where observed, open for a statistical claim,
 * dotted where reconstructed; in `colour`, ringed with the panel's colour. */
function Known({ known, colour, cx, cy }: { known: Known; colour: string; cx: number; cy: number }) {
  if (known === "observed") return <circle cx={cx} cy={cy} r={4} fill={colour} stroke="var(--color-panel)" strokeWidth={2} />;
  return <circle cx={cx} cy={cy} r={3.25} fill="var(--color-panel)" stroke={colour} strokeWidth={1.5} strokeDasharray={known === "reconstructed" ? "1.6 1.6" : undefined} data-known={known} />;
}

/** The ticks' gridlines along a row's axis. */
const Gridlines = () =>
  DEPTH_TICKS.map((t) => <line key={t.mpc} x1={depthX(t.mpc, AXIS)} x2={depthX(t.mpc, AXIS)} y1={2} y2={TRACK - 2} stroke="var(--color-line)" />);

/** A row's axis, with the structure's dot where it lies. */
function Track({ row }: { row: DepthRow }) {
  const at = row.distance === undefined ? undefined : Math.min(AXIS - 4, Math.max(4, depthX(row.distance, AXIS)));
  return (
    <svg width={AXIS} height={TRACK} viewBox={`0 0 ${AXIS} ${TRACK}`} aria-hidden="true" className="overflow-visible" data-testid="depth-track">
      <line x1={0} x2={AXIS} y1={TRACK / 2} y2={TRACK / 2} stroke="var(--color-line)" />
      <Gridlines />
      {at === undefined ? null : <Known known={row.known} colour={row.colour} cx={at} cy={TRACK / 2} />}
    </svg>
  );
}

/** One structure, to choose: its name and distance, then its kind and its place on the axis. */
function Row({ row, chosen, onChoose }: { row: DepthRow; chosen: boolean; onChoose: (id: string) => void }) {
  return (
    <li>
      <button
        type="button"
        aria-pressed={chosen}
        aria-label={`${row.name}, ${words(row.kind)}, ${KNOWN_WORDS[row.known]}, ${distanceText(row.distance)}`}
        onClick={() => {
          onChoose(row.id);
        }}
        className={`group grid w-full grid-cols-[1fr_auto] gap-x-2 rounded px-1.5 py-1 text-left ${chosen ? "bg-raised" : ""}`}
      >
        <span className={`font-serif text-[14.5px] leading-tight italic group-hover:text-accent ${chosen ? "font-medium" : ""}`}>{row.name}</span>
        <span className="text-xs whitespace-nowrap text-dim">{distanceText(row.distance)}</span>
        <span className="col-span-2 mt-0.5 flex items-center justify-between gap-2">
          <span className="flex min-w-0 items-center gap-1.5 text-xs text-dim">
            <span className="h-2 w-2 shrink-0 rounded-sm" style={{ background: row.colour }} />
            <span className="leading-tight">{words(row.kind)}</span>
          </span>
          <Track row={row} />
        </span>
      </button>
    </li>
  );
}

/** What the marks on the axis say of how a structure is known. */
const Legend = () => (
  <p className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-dim" data-testid="depth-legend">
    {(Object.keys(KNOWN_WORDS) as Known[]).map((known) => (
      <span key={known} className="inline-flex items-center gap-1">
        <svg width={10} height={10} aria-hidden="true">
          <Known known={known} colour="var(--color-ink)" cx={5} cy={5} />
        </svg>
        {KNOWN_WORDS[known]}
      </span>
    ))}
  </p>
);

/** The axis's ticks, labelled, over the rows' axes: inset by a row's own padding. */
const Ticks = () => (
  <svg width={AXIS} height={14} aria-hidden="true" className="mr-1.5 ml-auto block overflow-visible">
    {DEPTH_TICKS.map((t, i) => (
      <text key={t.mpc} x={depthX(t.mpc, AXIS)} y={10} fontSize="10.5" fill="var(--color-dim)" textAnchor={i === 0 ? "start" : i === DEPTH_TICKS.length - 1 ? "end" : "middle"}>
        {t.text}
      </text>
    ))}
  </svg>
);

type Order = "distance" | "name";

const ORDERS: readonly { order: Order; text: string }[] = [
  { order: "distance", text: "Nearest first" },
  { order: "name", text: "A to Z" },
];

/** The Structures drawer (see-the-structures): the structures outward from Earth, under headings
 * of distance, each placed on one logarithmic axis of distance that every row shares; or by name;
 * narrowed by what is typed. */
export function DepthIndex({ rows, chosen, onChoose }: { rows: readonly DepthRow[]; chosen: string | undefined; onChoose: (id: string) => void }) {
  const [text, setText] = useState("");
  const [order, setOrder] = useState<Order>("distance");
  const found = matching(rows, text);
  const groups = order === "distance" ? banded(found) : [{ heading: undefined, rows: byName(found) }];
  return (
    <div>
      <div className="sticky -top-3 z-10 -mx-3 -mt-3 grid gap-2 border-b border-line bg-panel px-3 pt-3 pb-2">
        <div className="flex items-baseline justify-between">
          <h2 className="font-serif text-base">Structures</h2>
          <span className="text-xs text-dim" data-testid="structures-count">
            {text.trim() === "" ? rows.length : `${found.length} of ${rows.length}`}
          </span>
        </div>
        <input
          type="search"
          id="find-a-structure"
          aria-label="Find a structure"
          placeholder="Find a structure"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
          className="w-full rounded border border-edge bg-space px-2 py-1 text-[13px] placeholder:text-dim"
        />
        <div role="group" aria-label="Order" className="inline-flex w-fit overflow-hidden rounded border border-edge text-xs">
          {ORDERS.map((o) => (
            <button
              key={o.order}
              type="button"
              aria-pressed={order === o.order}
              onClick={() => {
                setOrder(o.order);
              }}
              className={`px-2 py-0.5 ${order === o.order ? "bg-raised text-ink" : "text-dim hover:text-accent"}`}
            >
              {o.text}
            </button>
          ))}
        </div>
        <Legend />
        <Ticks />
      </div>
      <div data-testid="structures" className="mt-1">
        {rows.length === 0 ? (
          <p className="py-3 text-[13px] text-dim">No structures are drawn. Turn their kinds on in the Show drawer.</p>
        ) : found.length === 0 ? (
          <p className="py-3 text-[13px] text-dim">No structure's name or kind holds “{text.trim()}”. Try part of a name, or a kind such as void.</p>
        ) : (
          groups.map((group) => (
            <section key={group.heading ?? "all"} aria-label={group.heading ?? "Structures by name"} className="mt-2 border-t border-line first:mt-0 first:border-t-0">
              {group.heading === undefined ? null : (
                <h3 className="flex justify-between px-1.5 pt-2 pb-0.5 text-xs text-dim">
                  <span>{group.heading}</span>
                  <span>{group.rows.length}</span>
                </h3>
              )}
              <ul>
                {group.rows.map((row) => (
                  <Row key={row.id} row={row} chosen={row.id === chosen} onChoose={onChoose} />
                ))}
              </ul>
            </section>
          ))
        )}
      </div>
    </div>
  );
}
