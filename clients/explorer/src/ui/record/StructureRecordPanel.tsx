// The chosen structure's record panel (see-the-structures), on the base every record panel
// shares: what it is in one sentence, who disputes it, its depth, where it is on the sky, its
// lengths and other measures, how it is drawn, its papers named once, and the record's notes,
// quotes and every value under one link.

import { type ReactNode, useMemo } from "react";

import type { Cosmology } from "../../api/schemas.ts";
import { type StructureDetail, type StructureRecord, readPanelRecord } from "../../api/structures.ts";
import { structureName } from "../../scene/structures.ts";
import type { Inside } from "../inside.ts";
import { type AxisMark, DepthBand, LengthBars, TickStrip } from "./charts.tsx";
import { FullStructureRecord } from "./FullStructureRecord.tsx";
import { Disputed, Note, Rows, Section } from "./parts.tsx";
import { RecordPanel } from "./RecordPanel.tsx";
import {
  type DepthView,
  type MemberView,
  type SizeGroup,
  disputedBy,
  flowText,
  matchedText,
  measureLines,
  papersOf,
  skyView,
  structureLede,
  structureParts,
  structureSubtitle,
} from "./structureText.ts";

interface Props {
  detail: StructureDetail;
  /** Whether it is drawn by its bounding box. */
  approximate: boolean;
  /** The cull to its volume, where it has one. */
  inside?: Inside | undefined;
  /** The release cosmology, which places its velocities and rescales its lengths. */
  cosmology?: Cosmology | undefined;
  onClose: () => void;
}

/** The structure's record: its heading, sentence and parts, and every note, quote and value under
 * one link; or, where its record cannot be read, what could not be. */
export function StructureRecordPanel({ detail, approximate, inside, cosmology, onClose }: Props) {
  const read = useMemo(() => readPanelRecord(detail), [detail]);
  const shared = {
    label: "The chosen structure",
    title: structureName(detail),
    italic: true,
    closeLabel: "Close the structure's panel",
    onClose,
  };
  const cull = inside?.on === true ? <CullNote approximate={inside.approximate} /> : null;
  if ("problem" in read)
    return (
      <RecordPanel {...shared} subtitle={<span data-testid="structure-kind">{structureSubtitle({ ...detail, record: {} })}</span>}>
        <Note testid="structure-unread">Its record could not be read in full ({read.problem}), so only how it is drawn is shown.</Note>
        {cull}
        <Drawn detail={detail} approximate={approximate} />
      </RecordPanel>
    );
  return <ReadStructure s={read.structure} approximate={approximate} cull={cull} cosmology={cosmology} shared={shared} />;
}

function ReadStructure({
  s,
  approximate,
  cull,
  cosmology,
  shared,
}: {
  s: StructureRecord;
  approximate: boolean;
  cull: ReactNode;
  cosmology: Cosmology | undefined;
  shared: { label: string; title: string; italic: boolean; closeLabel: string; onClose: () => void };
}) {
  const parts = useMemo(() => structureParts(s, cosmology), [s, cosmology]);
  const disputed = disputedBy(s);
  return (
    <RecordPanel
      {...shared}
      subtitle={<span data-testid="structure-kind">{structureSubtitle(s)}</span>}
      lede={structureLede(s, parts)}
      fullLabel="Show the record's notes, quotes and every value"
      full={() => <FullStructureRecord s={s} approximate={approximate} />}
    >
      {disputed.length === 0 ? null : <Disputed by={disputed} />}
      {cull}
      <Depth band={parts.band} members={parts.members} />
      <Sky s={s} />
      <Size groups={parts.size} />
      <Measures s={s} used={[...(parts.band?.used ?? []), ...(parts.members?.used ?? [])]} />
      <Drawn detail={s} approximate={approximate} />
      <Papers s={s} />
    </RecordPanel>
  );
}

/** What the cull to the structure's volume leaves out, while it is on. */
function CullNote({ approximate }: { approximate: boolean }) {
  return (
    <Note testid="structure-inside">
      {approximate ? "Approximate: the cull is to a box about the structure, not its published shape. " : ""}
      Points are placed at their preferred distance, so members scattered past its faces are not drawn.
    </Note>
  );
}

const mpcText = (mpc: number) => `${mpc.toFixed(1)} Mpc`;

/** A depth chart's accessible name: its ends and each mark. */
const depthLabel = (start: string, view: DepthView, marks: readonly AxisMark[]) =>
  `${start} from ${mpcText(view.from)} to ${mpcText(view.to)}${marks.map((m) => `; ${m.label} at ${mpcText(m.value)}`).join("")}`;

/** Its depth as a band between its bounds, or its members as ticks. */
function Depth({ band, members }: { band: DepthView | undefined; members: MemberView | undefined }) {
  const view = band ?? members;
  if (view === undefined) return null;
  return (
    <Section title={band === undefined ? "Members" : "Depth"} testid="record-depth">
      {members === undefined || band !== undefined ? (
        <DepthBand from={view.from} to={view.to} marks={view.marks} unit="Mpc" label={depthLabel("Depth", view, view.marks)} />
      ) : (
        <TickStrip values={members.values} marks={members.marks} unit="Mpc" label={depthLabel(`${members.values.length} members`, members, members.marks)} />
      )}
      <Note>{view.note}</Note>
    </Section>
  );
}

/** Where it is on the sky, in its frame. */
function Sky({ s }: { s: StructureRecord }) {
  const sky = skyView(s);
  if (sky === undefined || sky.lines.length === 0) return null;
  return (
    <Section title="On the sky" testid="record-sky">
      <Rows lines={sky.lines} />
      <Note>{sky.note}</Note>
    </Section>
  );
}

/** Its lengths as bars, one axis for each unit, each with what it was published as. */
function Size({ groups }: { groups: readonly SizeGroup[] }) {
  if (groups.length === 0) return null;
  return (
    <Section title="Size" testid="record-size">
      {groups.map((g) => (
        <div key={g.unit} className="space-y-1">
          <LengthBars rows={g.rows} unit={g.unit} />
          {g.note === undefined ? null : <Note>{g.note}</Note>}
        </div>
      ))}
    </Section>
  );
}

/** Its other values, each on its line. */
function Measures({ s, used }: { s: StructureRecord; used: readonly string[] }) {
  const lines = measureLines(s, used);
  if (lines.length === 0) return null;
  return (
    <Section title="Measures" testid="record-measures">
      <Rows lines={lines} />
    </Section>
  );
}

/** How it is drawn on the view: its shape, and what its symbol or lines mean. */
function Drawn({ detail, approximate }: { detail: StructureDetail; approximate: boolean }) {
  const flow = flowText(detail.kind);
  return (
    <Section title="On the view" testid="record-drawn">
      <Note>Drawn as {matchedText(detail, approximate, false)}.</Note>
      {detail.derivation?.extents === undefined ? null : (
        <Note testid="structure-extents">Its bounds are unknown: the lines are its published extents about its position, which need not be its middle.</Note>
      )}
      {flow === undefined ? null : <Note testid="structure-flow">{flow}</Note>}
    </Section>
  );
}

/** Its papers, once each, by first author and year. */
function Papers({ s }: { s: StructureRecord }) {
  const papers = [...papersOf(s)];
  if (papers.length === 0) return null;
  return (
    <Section title="From" testid="record-papers">
      <ul className="space-y-0.5">
        {papers.map(([id, p]) => (
          <li key={id}>
            {p.short}
            {p.rest === "" ? null : <span className="text-dim">, {p.rest}</span>}
          </li>
        ))}
      </ul>
    </Section>
  );
}
