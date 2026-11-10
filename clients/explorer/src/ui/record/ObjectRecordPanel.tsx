// The chosen object's record panel (read-an-object), on the base every record panel shares: what
// it is in one sentence, its distances on one axis, where it is, its brightness and colour, its
// motion or velocity, its group, the structures it lies inside, its catalogues, and the whole
// record under one link.

import { useMemo } from "react";

import { type ObjectRecord, nameKey, placeOf } from "../../api/objects.ts";
import { whole } from "../../scene/format.ts";
import type { RecordContext } from "../ObjectPanel.tsx";
import { RecordBody } from "../RecordSections.tsx";
import { decText, designationText, epochText, galacticText, masText, namesAndDesignations, raText, skyOf } from "../recordText.ts";
import { KIND_NOUNS, headingOf } from "../text.ts";
import { useSources } from "../useSources.ts";
import { Mark } from "../Mark.tsx";
import { ColourScale, DotPlot, TickStrip } from "./charts.tsx";
import { Groups } from "./Groups.tsx";
import { sourceShort } from "./names.ts";
import { type Namer, brightnessLines, cataloguesText, distanceView, drawnColourOf, hasProperMotion, motionLines, objectLede, velocityView } from "./objectRecordText.ts";
import { Chooser, Lead, Note, Raised, Rows, Section } from "./parts.tsx";
import { RecordPanel } from "./RecordPanel.tsx";

interface Props {
  record: ObjectRecord;
  /** The name the object was found by, where it was found by one. */
  as?: string | undefined;
  context: RecordContext;
  onClose: () => void;
}

/** What each part reads: the record, its context, and the namer of sources. */
interface PartProps {
  record: ObjectRecord;
  context: RecordContext;
  name: Namer;
}

/** The object's record: its heading, sentence and parts, and every value under one link. */
export function ObjectRecordPanel({ record, as, context, onClose }: Props) {
  const read = useSources(context.release, context.sources === undefined ? record : undefined);
  const sources = context.sources ?? read;
  const name: Namer = (id) => sourceShort(id, sources.titles.get(id));
  const props: PartProps = { record, context, name };
  const heading = headingOf(record, as);
  return (
    <RecordPanel
      label="The chosen object"
      title={heading}
      subtitle={subtitleOf(record, heading)}
      lede={objectLede(record)}
      closeLabel="Close the object's panel"
      onClose={onClose}
      fullLabel="Show every name, value and flag, with the papers"
      full={() => <RecordBody record={record} context={{ ...context, sources }} />}
    >
      <Distance {...props} />
      <Position {...props} />
      <Brightness {...props} />
      <Motion {...props} />
      <Groups record={record} context={context} sources={sources} title={record.kind === "galaxy_group" ? "Members" : "Groups"} name={name} />
      <LiesInside {...props} />
      <Catalogues {...props} />
    </RecordPanel>
  );
}

/** Its proper names and its designations other than its heading, up to two, then its kind where
 * it is a group, and its id. A designation is left out where it gives a number already shown, as
 * Gaia EDR3's repeats Gaia DR3's, or a crossmatch gives the Hipparcos number of the heading. */
function subtitleOf(record: ObjectRecord, heading: string): string {
  const shown = new Set([nameKey(heading)]);
  for (const d of record.designations) if (designationText(d) === heading) shown.add(nameKey(d.identifier));
  const others = namesAndDesignations(record).names.map((n) => n.text).filter((t) => t !== heading);
  for (const d of record.designations) {
    const keys = [nameKey(d.identifier), nameKey(designationText(d))];
    if (keys.some((k) => shown.has(k))) continue;
    for (const k of keys) shown.add(k);
    others.push(d.designation ?? `${d.identifier} (${d.source})`);
  }
  return [...others.slice(0, 2), ...[KIND_NOUNS[record.kind]].filter((noun) => noun !== undefined), `object ${record.id}`].join(" · ");
}

/** The preferred distance large, every distance on one axis, and what preferred it. */
function Distance({ record, name }: PartProps) {
  const view = distanceView(record, name);
  if (view === undefined) return null;
  return (
    <Section title="Distance" testid="record-distance">
      <Lead figure={view.figure} kind={view.kind}>
        {view.minus === undefined ? null : <Raised>{view.minus === view.plus ? `± ${view.plus}` : `+${view.plus} −${view.minus}`}</Raised>} {view.unit}{" "}
        <span className="text-dim">· {view.aside}</span>
      </Lead>
      {view.rows.length === 0 ? null : <DotPlot rows={view.rows} unit={view.unit} />}
      <Note testid="distance-rule">
        {view.rule}
        {view.parallaxes.length === 0 ? null : (
          <>
            {" "}
            Parallaxes are drawn at their inverse:{" "}
            {view.parallaxes.map((p, i) => (
              <span key={p.text}>
                {i === 0 ? "" : ", "}
                {p.text} <Mark kind={p.kind} />
              </span>
            ))}
            .
          </>
        )}
      </Note>
    </Section>
  );
}

/** Where the object is on the sky, its frame, epoch and source said once beneath. */
function Position({ record, context, name }: PartProps) {
  const sky = skyOf(record, context.toGalactic);
  if (sky === undefined) return null;
  const { galactic, from } = sky;
  // A catalogue's position is measured, and the galactic is the same position in another frame.
  const lines = [
    { label: "RA, Dec", value: `${raText(sky.ra)}, ${decText(sky.dec)}`, kind: "measured" },
    ...(galactic === undefined ? [] : [{ label: "Galactic", value: galacticText(galactic, 3), kind: "measured" }]),
  ];
  return (
    <Section title="Position" testid="record-position">
      <Rows lines={lines} />
      <Note>
        {sky.frame.toUpperCase()}, {epochText(sky.epoch)}, {masText(sky.uncertainty_mas)}
        {from === undefined ? "" : `, ${name(from.source)}`}.
      </Note>
    </Section>
  );
}

/** Its magnitudes, colour indices and types, and the colour index it is drawn by on the colour
 * scale stars are drawn in. */
function Brightness({ record, name }: PartProps) {
  const lines = brightnessLines(record, name);
  const colour = drawnColourOf(record);
  if (lines.length === 0) return null;
  return (
    <Section title="Brightness and colour" testid="record-brightness">
      <Rows lines={lines} />
      {colour === undefined ? null : <ColourScale colour={colour} />}
    </Section>
  );
}

/** A star's proper motion and radial velocity; any other object's velocities, many from one
 * source drawn as ticks. */
function Motion({ record, name }: PartProps) {
  if (hasProperMotion(record))
    return (
      <Section title="Motion" testid="record-motion">
        <Rows lines={motionLines(record, name)} />
      </Section>
    );
  const { lines, strip } = velocityView(record, name);
  if (lines.length === 0 && strip === undefined) return null;
  return (
    <Section title="Velocity" testid="record-motion">
      <Rows lines={lines} />
      {strip === undefined ? null : (
        <>
          <TickStrip
            values={strip.values}
            marks={strip.mark === undefined ? [] : [{ value: strip.mark, label: whole(strip.mark) }]}
            unit="km/s"
            label={`${strip.values.length} redshifts from ${strip.source} as cz, from ${whole(Math.min(...strip.values))} to ${whole(Math.max(...strip.values))} km/s`}
          />
          <Note>
            {strip.values.length.toLocaleString("en-GB")} {strip.source} redshifts as cz <Mark kind="measured" />
            {strip.mark === undefined ? "" : ", marked with the velocity another source gives"}.
          </Note>
        </>
      )}
    </Section>
  );
}

/** The structures of the release whose volume holds the object, each to choose. */
function LiesInside({ record, context }: PartProps) {
  const { holding: holdingOf } = context;
  const holding = useMemo(() => {
    const place = placeOf(record);
    return place === undefined ? [] : holdingOf(place);
  }, [record, holdingOf]);
  if (holding.length === 0) return null;
  return (
    <Section title="Lies inside" testid="record-inside">
      <p>
        {holding.map((s, i) => (
          <span key={s.id}>
            {i === 0 ? "" : ", "}
            <Chooser
              italic
              onClick={() => {
                context.onChooseStructure(s.id);
              }}
            >
              {s.name}
            </Chooser>
          </span>
        ))}
      </p>
    </Section>
  );
}

/** How many catalogues hold the object, and how many entries in each. */
function Catalogues({ record, name }: PartProps) {
  const { title, detail } = cataloguesText(record, name);
  return (
    <Section title={title} testid="catalogues-summary">
      <Note>{detail}</Note>
    </Section>
  );
}
