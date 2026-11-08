// The chosen structure's record panel (see-the-structures), on the base every record panel
// shares: what it is in one sentence, who disputes it, its depth, where it is on the sky, its
// lengths and other measures, how it is drawn, its papers named once, and the record's notes,
// quotes and every value under one link.

import type { Cosmology } from "../../api/schemas.ts";
import type { StructureDetail } from "../../api/structures.ts";
import { structureName } from "../../scene/structures.ts";
import type { Inside } from "../inside.ts";
import { DepthBand, LengthBars, TickStrip } from "./charts.tsx";
import { FullStructureRecord } from "./FullStructureRecord.tsx";
import { Disputed, Note, Rows, Section } from "./parts.tsx";
import { RecordPanel } from "./RecordPanel.tsx";
import {
  depthBand,
  disputedBy,
  flowText,
  matchedText,
  measureLines,
  memberStrip,
  papersOf,
  sizeView,
  skyView,
  structureLede,
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

/** What each part reads. */
interface PartProps {
  detail: StructureDetail;
  cosmology: Cosmology | undefined;
}

/** The structure's record: its heading, sentence and parts, and every note, quote and value under
 * one link. */
export function StructureRecordPanel({ detail, approximate, inside, cosmology, onClose }: Props) {
  const band = depthBand(detail, cosmology);
  const members = memberStrip(detail, cosmology);
  const disputed = disputedBy(detail);
  return (
    <RecordPanel
      label="The chosen structure"
      title={structureName(detail)}
      italic
      subtitle={<span data-testid="structure-kind">{structureSubtitle(detail)}</span>}
      lede={structureLede(detail, cosmology)}
      closeLabel="Close the structure's panel"
      onClose={onClose}
      fullLabel="Show the record's notes, quotes and every value"
      full={() => <FullStructureRecord detail={detail} approximate={approximate} />}
    >
      {disputed.length === 0 ? null : <Disputed by={disputed} />}
      {inside?.on === true ? <CullNote approximate={inside.approximate} /> : null}
      <Depth band={band} members={members} />
      <Sky detail={detail} cosmology={cosmology} />
      <Size detail={detail} cosmology={cosmology} />
      <Measures detail={detail} used={[...(band?.used ?? []), ...(members?.used ?? [])]} />
      <Drawn detail={detail} approximate={approximate} />
      <Papers detail={detail} cosmology={cosmology} />
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

const PC = (mpc: number) => `${mpc.toFixed(1)} Mpc`;

/** Its depth as a band between its bounds, or its members as ticks. */
function Depth({ band, members }: { band: ReturnType<typeof depthBand>; members: ReturnType<typeof memberStrip> }) {
  if (band !== undefined)
    return (
      <Section title="Depth" testid="record-depth">
        <DepthBand
          from={band.from}
          to={band.to}
          marks={band.marks}
          unit="Mpc"
          label={`From ${PC(band.from)} to ${PC(band.to)}${band.marks.map((m) => `; ${m.label} at ${PC(m.value)}`).join("")}`}
        />
        <Note>{band.note}</Note>
      </Section>
    );
  if (members === undefined) return null;
  return (
    <Section title="Members" testid="record-depth">
      <TickStrip
        values={members.values}
        marks={members.marks}
        unit="Mpc"
        label={`${members.values.length} members from ${PC(members.from)} to ${PC(members.to)}${members.marks.map((m) => `; ${m.label} at ${PC(m.value)}`).join("")}`}
      />
      <Note>{members.note}</Note>
    </Section>
  );
}

/** Where it is on the sky, in its frame. */
function Sky({ detail }: PartProps) {
  const sky = skyView(detail);
  if (sky === undefined || sky.lines.length === 0) return null;
  return (
    <Section title="On the sky" testid="record-sky">
      <Rows lines={sky.lines} />
      <Note>{sky.note}</Note>
    </Section>
  );
}

/** Its lengths as bars, one axis for each unit, each with what it was published as. */
function Size({ detail, cosmology }: PartProps) {
  const groups = sizeView(detail, cosmology);
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
function Measures({ detail, used }: { detail: StructureDetail; used: readonly string[] }) {
  const lines = measureLines(detail, used);
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
function Papers({ detail }: PartProps) {
  const papers = [...papersOf(detail)];
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
