// The record panels' charts (read-an-object, see-the-structures), where a chart says more than a
// list: distances on one axis, many velocities as ticks, a depth as a band, lengths as bars, a
// colour on the scale stars are drawn in. Each is a 320-pixel SVG whose accessible name carries
// its values; its text is in the ink or dim, never in a data colour.

import { figures, whole } from "../../scene/format.ts";
import { KEY_TEMPERATURES, STAR_GRADIENT, type StarColourIndex, temperatureOf } from "../../scene/starColour.ts";
import { bandsText } from "../text.ts";
import { Mark } from "../Mark.tsx";
import { labelRows, linear, niceDomain } from "./chartGeometry.ts";

const W = 320;
const LINE = "var(--color-line)";
const DIM = "var(--color-dim)";
const INK = "var(--color-ink)";
const ACCENT = "var(--color-accent)";
const NEAT = "var(--color-neat)";
const PANEL = "var(--color-panel)";

/** A tick's number: whole where it is large, to three figures where it is small. */
const number = (v: number): string => (Math.abs(v) >= 1000 ? whole(v) : figures(v, 3));

/** An axis along `y`, ticks at `ticks` placed by `x`, gridlines up to `top`, the unit after the
 * last tick. */
function Axis({ ticks, x, y, top, unit }: { ticks: readonly number[]; x: (v: number) => number; y: number; top: number; unit: string }) {
  const last = ticks.length - 1;
  return (
    <g fontSize="11" fill={DIM}>
      <line x1={x(ticks[0] ?? 0)} y1={y} x2={x(ticks[last] ?? 0)} y2={y} stroke={LINE} />
      {ticks.map((t, i) => (
        <g key={t}>
          <line x1={x(t)} y1={top} x2={x(t)} y2={y} stroke={LINE} />
          <text x={x(t)} y={y + 14} textAnchor={i === 0 ? "start" : i === last ? "end" : "middle"}>
            {number(t)}
            {i === last ? ` ${unit}` : ""}
          </text>
        </g>
      ))}
    </g>
  );
}

/** A dot with the panel's ring about it, filled or open. */
function Dot({ cx, cy, colour, open = false }: { cx: number; cy: number; colour: string; open?: boolean }) {
  return <circle cx={cx} cy={cy} r={4.5} fill={open ? PANEL : colour} stroke={open ? colour : PANEL} strokeWidth={2} />;
}

/** One measurement of a dot plot: its label, value and uncertainty, and whether it is the one
 * preferred; `text` says it in words for the plot's accessible name. */
export interface DotRow {
  label: string;
  value: number;
  low?: number | undefined;
  high?: number | undefined;
  preferred?: boolean;
  text: string;
}

const ROW = 30;

/** Measurements of one quantity as dots on one axis, each with its uncertainty and its label above
 * it, the preferred in the accent and the rest grey. */
export function DotPlot({ rows, unit }: { rows: readonly DotRow[]; unit: string }) {
  const { domain, ticks } = niceDomain(
    rows.flatMap((r) => [r.low ?? r.value, r.high ?? r.value]),
    4,
  );
  const x = linear(domain, [6, W - 12]);
  const height = rows.length * ROW + 26;
  return (
    <svg role="img" aria-label={rows.map((r) => `${r.label}: ${r.text}`).join("; ")} width={W} height={height} viewBox={`0 0 ${W} ${height}`} data-testid="dot-plot">
      <Axis ticks={ticks} x={x} y={height - 22} top={0} unit={unit} />
      {rows.map((r, i) => {
        const y = 21 + i * ROW;
        const colour = r.preferred === true ? ACCENT : DIM;
        return (
          <g key={`${r.label}:${i}`}>
            <text x={0} y={y - 9} fontSize="12" fill={r.preferred === true ? INK : DIM}>
              {r.label}
            </text>
            {r.low === undefined || r.high === undefined ? null : <line x1={x(r.low)} y1={y} x2={x(r.high)} y2={y} stroke={colour} strokeWidth={2} strokeLinecap="round" />}
            <Dot cx={x(r.value)} cy={y} colour={colour} />
          </g>
        );
      })}
    </svg>
  );
}

/** A named point on an axis: a velocity, a median, a wall. */
export interface AxisMark {
  value: number;
  label: string;
}

/** The height labels above an axis take: two rows of them. */
const LABELS = 28;

/** Named marks' labels above an axis, a label that would meet another set on a second row. */
function MarkLabels({ marks, x }: { marks: readonly AxisMark[]; x: (v: number) => number }) {
  const rows = labelRows(marks.map((m) => x(m.value)));
  return (
    <>
      {marks.map((m, i) => (
        <text key={m.label} x={x(m.value)} y={11 + (rows[i] ?? 0) * 13} fontSize="12" fill={INK} textAnchor="middle">
          {m.label}
        </text>
      ))}
    </>
  );
}

/** Many values as thin ticks on one axis, each named mark in the accent above them. */
export function TickStrip({ values, marks = [], unit, label }: { values: readonly number[]; marks?: readonly AxisMark[]; unit: string; label: string }) {
  const { domain, ticks } = niceDomain([...values, ...marks.map((m) => m.value)], 4);
  const x = linear(domain, [4, W - 8]);
  const top = LABELS;
  return (
    <svg role="img" aria-label={label} width={W} height={top + 48} viewBox={`0 0 ${W} ${top + 48}`} data-testid="tick-strip">
      <Axis ticks={ticks} x={x} y={top + 28} top={top} unit={unit} />
      {values.map((v, i) => (
        <line key={i} x1={x(v)} y1={top + 6} x2={x(v)} y2={top + 22} stroke={NEAT} strokeWidth={1.5} strokeOpacity={0.7} />
      ))}
      {marks.map((m) => (
        <line key={m.label} x1={x(m.value)} y1={top} x2={x(m.value)} y2={top + 26} stroke={ACCENT} strokeWidth={2} strokeLinecap="round" />
      ))}
      <MarkLabels marks={marks} x={x} />
    </svg>
  );
}

/** A range on one axis as a band between its ends, with dots at named points within it. */
export function DepthBand({ from, to, marks = [], unit, label }: { from: number; to: number; marks?: readonly AxisMark[]; unit: string; label: string }) {
  const { domain, ticks } = niceDomain([from, to], 4);
  const x = linear(domain, [6, W - 8]);
  const top = LABELS;
  return (
    <svg role="img" aria-label={label} width={W} height={top + 48} viewBox={`0 0 ${W} ${top + 48}`} data-testid="depth-band">
      <Axis ticks={ticks} x={x} y={top + 28} top={top - 6} unit={unit} />
      <rect x={x(from)} y={top + 2} width={Math.max(2, x(to) - x(from))} height={16} rx={4} fill={ACCENT} fillOpacity={0.12} />
      {[from, to].map((v) => (
        <line key={v} x1={x(v)} y1={top + 2} x2={x(v)} y2={top + 18} stroke={ACCENT} strokeWidth={2} strokeLinecap="round" />
      ))}
      {marks.map((m) => (
        <Dot key={m.label} cx={x(m.value)} cy={top + 10} colour={ACCENT} />
      ))}
      <MarkLabels marks={marks} x={x} />
    </svg>
  );
}

/** One length of a structure: its label, value, whether it is a lower limit, the words for it,
 * and its kind of value. */
export interface LengthRow {
  label: string;
  value: number;
  open: boolean;
  text: string;
  kind: string | undefined;
}

const BAR = 10;
const BAR_STEP = 34;

/** Lengths of one unit as bars from zero on one axis, each beneath its label; a lower limit fades
 * out past its end. */
export function LengthBars({ rows, unit }: { rows: readonly LengthRow[]; unit: string }) {
  const { domain, ticks } = niceDomain([0, ...rows.map((r) => r.value)], 3);
  const x = linear(domain, [0, W - 56]);
  const height = rows.length * BAR_STEP + 24;
  const id = `open-${unit.replaceAll(/\W/g, "")}`;
  return (
    <div className="relative" data-testid="length-bars">
      <svg role="img" aria-label={rows.map((r) => `${r.label}: ${r.text} ${unit}`).join("; ")} width={W} height={height} viewBox={`0 0 ${W} ${height}`}>
        <defs>
          <linearGradient id={id} x1="0" x2="1">
            <stop offset="0.8" stopColor={NEAT} />
            <stop offset="1" stopColor={NEAT} stopOpacity={0} />
          </linearGradient>
        </defs>
        <Axis ticks={ticks} x={x} y={height - 20} top={0} unit={unit} />
        {rows.map((r, i) => {
          const y = 18 + i * BAR_STEP;
          const end = x(r.value);
          return (
            <g key={r.label}>
              <text x={0} y={y - 5} fontSize="12.5" fill={DIM}>
                {r.label}
              </text>
              <rect x={0} y={y} width={Math.max(2, end + (r.open ? 18 : 0))} height={BAR} rx={2} fill={r.open ? `url(#${id})` : NEAT} />
              <text x={end + (r.open ? 22 : 5)} y={y + 9} fontSize="12.5" fill={INK}>
                {r.text}
              </text>
            </g>
          );
        })}
      </svg>
      {rows.map((r, i) => (
        <span key={r.label} className="absolute right-0" style={{ top: 2 + i * BAR_STEP }}>
          <Mark kind={r.kind} />
        </span>
      ))}
    </div>
  );
}

/** Where temperature `teff` lies along the key's gradient, 0 to 1: its stops are evenly spaced
 * at the key's temperatures, and between two it moves in log temperature. */
export function keyPosition(teff: number): number {
  const keys = KEY_TEMPERATURES;
  const last = keys.length - 1;
  const above = keys.findIndex((k) => k >= teff);
  if (above <= 0) return above === 0 ? 0 : 1;
  const lo = keys[above - 1] ?? teff;
  const hi = keys[above] ?? teff;
  return (above - 1 + Math.log(teff / lo) / Math.log(hi / lo)) / last;
}

/** The gradient the view draws stars in, with its temperatures beneath, and a mark at `at` (0 to
 * 1) where one is given: the key's scale and a star's place on it alike. */
export function StarScale({ at, height = "h-3" }: { at?: number | undefined; height?: string }) {
  return (
    <>
      <div className={`relative ${height} rounded`} style={{ background: STAR_GRADIENT }}>
        {at === undefined ? null : <span className="absolute -top-1 h-[calc(100%+8px)] w-0.5 rounded bg-ink ring-2 ring-panel" style={{ left: `${(at * 100).toFixed(1)}%` }} />}
      </div>
      <div className="flex justify-between text-xs text-dim">
        {KEY_TEMPERATURES.map((t) => (
          <span key={t}>{t.toLocaleString("en-GB")} K</span>
        ))}
      </div>
    </>
  );
}

/** A star's colour index, the one the view draws it by, on the gradient stars are drawn in, with
 * the temperature and class it gives along the dwarf sequence. */
export function ColourScale({ colour }: { colour: StarColourIndex }) {
  const { teff, type } = temperatureOf(colour.index, colour.bands);
  return (
    <div role="img" aria-label={`${bandsText(colour.bands)} ${figures(colour.index, 4)}: about ${whole(teff)} K, ${type} on the dwarf sequence`} className="space-y-1" data-testid="colour-scale">
      <StarScale at={keyPosition(teff)} height="h-2" />
    </div>
  );
}
