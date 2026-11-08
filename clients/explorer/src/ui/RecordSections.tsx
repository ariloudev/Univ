import { type ReactNode, useState } from "react";

import { type Found, type ObjectRecord, distancesOf } from "../api/objects.ts";
import { minusSign } from "../scene/format.ts";
import { Mark } from "./Mark.tsx";
import { Chooser } from "./record/parts.tsx";
import type { RecordContext } from "./ObjectPanel.tsx";
import {
  decText,
  designationText,
  flagsOf,
  formedAs,
  skyOf,
  epochText,
  galacticText,
  masText,
  namesAndDesignations,
  raText,
  relationText,
  relationsByOther,
} from "./recordText.ts";
import { colourIndices, estimatedClass, publishedTypes } from "./starText.ts";
import { bandsText, natureOf, preferredText, valueText, words } from "./text.ts";
import { type Sources, useSources } from "./useSources.ts";

/** How many items a long list shows before the rest fold away. */
const SHOWN = 6;

/** What each part reads: the record, its context, and the release's sources. */
interface PartProps {
  record: ObjectRecord;
  context: RecordContext;
  sources: Sources;
}

/** A source's title, or its id where the title is not known. */
const titleIn = (sources: Sources) => (id: string) => sources.titles.get(id) ?? id;

/** A part of the record under its heading. */
function Part({ title, testid, children }: { title: string; testid: string; children: ReactNode }) {
  return (
    <div data-testid={testid}>
      <h3 className="mb-0.5 text-xs text-dim">{title}</h3>
      {children}
    </div>
  );
}

/** `items` as a list, those past `SHOWN` folded under a line that counts them and made only once
 * it is opened. */
export function Folded({ items }: { items: ReactNode[] }) {
  const [open, setOpen] = useState(false);
  if (items.length <= SHOWN) return <ul className="space-y-0.5">{items}</ul>;
  return (
    <ul className="space-y-0.5">
      {items.slice(0, SHOWN)}
      <li>
        <details
          onToggle={(event) => {
            setOpen(event.currentTarget.open);
          }}
        >
          <summary className="cursor-pointer text-dim hover:text-accent">{items.length - SHOWN} more</summary>
          {open ? <ul className="mt-0.5 space-y-0.5">{items.slice(SHOWN)}</ul> : null}
        </details>
      </li>
    </ul>
  );
}

/** A value's line: the mark of its kind, the value, and a note of its kind and source. */
function ValueLine({ kind, children, note }: { kind: string | undefined; children: ReactNode; note: string }) {
  return (
    <li>
      <Mark kind={kind} />
      {children} <span className="text-dim">({note})</span>
    </li>
  );
}


/** The whole record of a ready object, as its panel's link opens it: every name, value, catalogue
 * entry with its flags, and paper. Its groups and the structures it lies inside the panel shows
 * itself. */
export function RecordBody({ record, context }: { record: ObjectRecord; context: RecordContext }) {
  const read = useSources(context.release, context.sources === undefined ? record : undefined);
  const props: PartProps = { record, context, sources: context.sources ?? read };
  return (
    <div className="space-y-3">
      <p className="text-dim" data-testid="nature">
        {natureOf(record)}
      </p>
      <Names {...props} />
      <Sky {...props} />
      <Distance {...props} />
      <Values {...props} />
      <Star {...props} />
      <Catalogues {...props} />
      <Papers {...props} />
      <Frame record={record} />
    </div>
  );
}

/** The object's proper names, then its designations, each written once with every source that
 * gives it. */
function Names({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const { names, designations } = namesAndDesignations(record);
  const items = [
    ...names.map((n) => (
      <li key={`n:${n.text}`}>
        <span className="font-serif">{n.text}</span> <span className="text-dim">({n.sources.map(title).join("; ")})</span>
      </li>
    )),
    ...designations.map((d) => (
      <li key={`d:${d.text}:${d.sources.join()}`}>
        {d.text} <span className="text-dim">({d.sources.map(title).join("; ")})</span>
      </li>
    )),
  ];
  return (
    <Part title="Names and designations" testid="names">
      <Folded items={items} />
    </Part>
  );
}


/** Where the object is on the sky: right ascension and declination, and galactic longitude and
 * latitude, each with its frame and epoch, and the entry the position is taken from. */
function Sky({ record, context, sources }: PartProps) {
  const sky = skyOf(record, context.toGalactic);
  if (sky === undefined) return null;
  const { galactic, from } = sky;
  return (
    <Part title="On the sky" testid="sky">
      <ul className="space-y-0.5">
        <li>
          RA {raText(sky.ra)}, Dec {decText(sky.dec)}{" "}
          <span className="text-dim">
            ({sky.frame.toUpperCase()}, {epochText(sky.epoch)}, {masText(sky.uncertainty_mas)})
          </span>
        </li>
        {galactic === undefined ? null : (
          <li>
            {galacticText(galactic)} <span className="text-dim">(galactic, from that position, {epochText(sky.epoch)})</span>
          </li>
        )}
      </ul>
      {from === undefined ? null : <p className="text-xs text-dim">From {titleIn(sources)(from.source)}</p>}
    </Part>
  );
}

/** The preferred distance, its rule, and every other distance measurement beside it. */
function Distance({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const { preferred, others } = distancesOf(record);
  const chosen = record.preferred;
  return (
    <Part title="Distance" testid="preferred">
      {chosen === null ? (
        <p className="text-dim">
          No distance: the object has a sky position only
          {record.not_placed === null || record.not_placed === undefined ? "" : ` (${words(record.not_placed)})`}, so it is not drawn.
        </p>
      ) : (
        <>
          <p>
            <Mark kind={preferred?.kind_of_value} />
            {preferredText(chosen)}
          </p>
          <p className="text-xs text-dim">
            {preferred?.kind_of_value ?? "unknown kind"}, {preferred === undefined ? "unknown source" : title(preferred.source)}; chosen by the rule “
            {words(chosen.rule)}”
          </p>
        </>
      )}
      {others.length === 0 ? null : (
        <ul data-testid="other-distances" className="mt-1 space-y-0.5">
          {others.map((m) => (
            <ValueLine key={`${m.entry}:${m.quantity}:${m.n}`} kind={m.kind_of_value} note={`${words(m.quantity)}, ${m.kind_of_value}, ${title(m.source)}`}>
              {valueText(m.native)}
            </ValueLine>
          ))}
        </ul>
      )}
    </Part>
  );
}

/** What each measured quantity of the motion parts is called. */
const MOTION: Record<string, string> = {
  proper_motion_ra: "Proper motion μα*",
  proper_motion_dec: "Proper motion μδ",
  velocity: "Radial velocity",
  redshift: "Redshift",
};

/** The magnitudes, and the proper motion, radial velocity and redshift, where sources publish
 * them; a redshift also as z where Univ Server normalized it. */
function Values({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const magnitudes = record.measurements.filter((m) => m.quantity === "magnitude");
  const motion = record.measurements.filter((m) => Object.hasOwn(MOTION, m.quantity));
  const line = (m: (typeof magnitudes)[number], label: string) => {
    const z = m.quantity === "redshift" && m.normalized?.unit === "dimensionless" ? `, z = ${minusSign(m.normalized.value.toPrecision(4))}` : "";
    return (
      <ValueLine key={`${m.entry}:${m.quantity}:${m.n}`} kind={m.kind_of_value} note={`${m.kind_of_value}, ${title(m.source)}`}>
        {label} {valueText(m.native)}
        {z}
      </ValueLine>
    );
  };
  return (
    <>
      {magnitudes.length === 0 ? null : (
        <Part title="Brightness" testid="brightness">
          <Folded items={magnitudes.map((m) => line(m, m.band ?? "Magnitude"))} />
        </Part>
      )}
      {motion.length === 0 ? null : (
        <Part title="Motion and redshift" testid="motion">
          <Folded items={motion.map((m) => line(m, MOTION[m.quantity] ?? words(m.quantity)))} />
        </Part>
      )}
    </>
  );
}

/** A star's published spectral types, its colour indices, and the class its colour index
 * suggests where no source publishes a type. */
function Star({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const types = publishedTypes(record);
  const colours = colourIndices(record);
  const estimated = estimatedClass(types, colours);
  if (types.length === 0 && colours.length === 0) return null;
  return (
    <Part title="Spectral type and colour" testid="star">
      <ul className="space-y-0.5">
        {types.map((t) => (
          <ValueLine key={`t:${t.source}:${t.text}`} kind={t.kind} note={`${t.kind}, ${title(t.source)}`}>
            {t.text}
          </ValueLine>
        ))}
        {estimated === undefined ? null : (
          <li>
            <Mark kind="estimated" />
            Estimated class {estimated}
          </li>
        )}
        {colours.map((m) => (
          <ValueLine
            key={`c:${m.entry}:${m.n}`}
            kind={m.kind_of_value}
            note={`${m.kind_of_value}, ${title(m.source)}${m.derived_from === null || m.derived_from === undefined ? "" : `, derived from ${m.derived_from.join(" − ")}`}`}
          >
            {bandsText(m.band ?? "colour index")} {valueText(m.native)}
          </ValueLine>
        ))}
      </ul>
    </Part>
  );
}


/** Each catalogue entry of the object, with its flags and what their values mean. */
function Catalogues({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const items = record.entries.map((e) => {
    const flags = flagsOf(e, sources.details.get(e.source));
    return (
      <li key={`${e.source}:${e.identifier}`}>
        <span>{title(e.source)}</span> <span className="text-dim">{designationText(e)}</span>
        {flags.length === 0 ? null : (
          <ul className="ml-3 text-xs">
            {flags.map((f) => (
              <li key={f.column}>
                <span className="text-dim">{f.column}</span> {f.value}
                {f.meaning === undefined ? null : <span title={f.cite}>: {f.meaning}</span>}
              </li>
            ))}
          </ul>
        )}
      </li>
    );
  });
  return (
    <Part title="Catalogues" testid="catalogues">
      <Folded items={items} />
    </Part>
  );
}

/** The papers each researched entry's values come from. */
function Papers({ record, sources }: PartProps) {
  const title = titleIn(sources);
  const researched = record.entries.filter((e) => (e.references ?? []).length > 0);
  if (researched.length === 0) return null;
  return (
    <Part title="Papers of the researched values" testid="papers">
      <ul className="space-y-0.5">
        {researched.flatMap((e) =>
          (e.references ?? []).map((r) => (
            <li key={`${e.source}:${r.citation}`}>
              {r.citation}{" "}
              <span className="text-dim">({[r.bibcode, r.doi, title(e.source)].filter((x) => x !== null && x !== undefined).join(", ")})</span>
            </li>
          )),
        )}
      </ul>
    </Part>
  );
}

function Frame({ record }: { record: ObjectRecord }) {
  const position = record.position;
  const sky = record.sky_position;
  const text =
    position !== null
      ? `${position.frame}, about the Solar System barycentre, epoch ${position.epoch}`
      : sky === null || sky === undefined
        ? "no position"
        : `${sky.frame} sky position, epoch ${sky.epoch ?? "not given"}`;
  return (
    <p className="text-xs text-dim" data-testid="position-frame">
      Position: {text}
    </p>
  );
}
