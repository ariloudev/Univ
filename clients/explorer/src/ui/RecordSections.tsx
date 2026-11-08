import type { ReactNode } from "react";

import { type Measurement, type ObjectRecord, distancesOf, placeOf } from "../api/objects.ts";
import { Mark } from "./Mark.tsx";
import type { RecordContext } from "./ObjectPanel.tsx";
import { decText, designationText, flagsOf, galacticOf, mergedDesignations, raText, relationText, relationsByOther } from "./recordText.ts";
import { colourIndices, estimatedClass, publishedTypes } from "./starText.ts";
import { bandsText, natureOf, preferredText, valueText, words } from "./text.ts";

/** How many items a long list shows before the rest fold away. */
const SHOWN = 6;

/** A part of the record under its heading. */
function Part({ title, testid, children }: { title: string; testid: string; children: ReactNode }) {
  return (
    <div data-testid={testid}>
      <h3 className="mb-0.5 text-xs text-dim">{title}</h3>
      {children}
    </div>
  );
}

/** `items` as a list, those past `SHOWN` folded under a line that counts them. */
function Folded({ items }: { items: ReactNode[] }) {
  if (items.length <= SHOWN) return <ul className="space-y-0.5">{items}</ul>;
  return (
    <ul className="space-y-0.5">
      {items.slice(0, SHOWN)}
      <li>
        <details>
          <summary className="cursor-pointer text-dim hover:text-accent">{items.length - SHOWN} more</summary>
          <ul className="mt-0.5 space-y-0.5">{items.slice(SHOWN)}</ul>
        </details>
      </li>
    </ul>
  );
}

/** The record of a ready object: what is known of it, part by part. */
export function RecordBody({ record, context }: { record: ObjectRecord; context: RecordContext }) {
  const title = (id: string) => context.sources.titles.get(id) ?? id;
  return (
    <div className="space-y-3">
      <p className="text-dim" data-testid="nature">
        {natureOf(record)}
      </p>
      <Names record={record} title={title} />
      <Sky record={record} context={context} title={title} />
      <Distance record={record} title={title} />
      <Values record={record} title={title} />
      <Star record={record} title={title} />
      <Groups record={record} context={context} title={title} />
      <Inside record={record} context={context} />
      <Catalogues record={record} context={context} title={title} />
      <Papers record={record} />
      <Frame record={record} />
    </div>
  );
}

type Titled = { record: ObjectRecord; title: (id: string) => string };

/** Whether every name in an entry's `name`, which may list several separated by commas, is among
 * the object's names or designations already, as a Hipparcos number a Gaia entry gives is. */
function named(record: ObjectRecord, name: string): boolean {
  const known = new Set([...(record.names ?? []).map((n) => n.name), ...record.designations.flatMap((d) => [d.identifier, designationText(d)])]);
  return name.split(",").every((part) => known.has(part.trim()));
}

/** The object's proper names, then its designations, each written once with every source that
 * gives it. */
function Names({ record, title }: Titled) {
  const items = [
    ...(record.names ?? []).map((n) => (
      <li key={`n:${n.source}:${n.name}`}>
        <span className="font-serif">{n.name}</span> <span className="text-dim">({title(n.source)})</span>
      </li>
    )),
    ...record.entries
      .filter((e) => e.name !== null && !named(record, e.name))
      .map((e) => (
        <li key={`e:${e.source}:${e.identifier}`}>
          <span className="font-serif">{e.name}</span> <span className="text-dim">({title(e.source)})</span>
        </li>
      )),
    ...mergedDesignations(record).map((d) => (
      <li key={`d:${d.text}`}>
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

/** The epoch of a position, as a Julian year. */
const epochText = (epoch: number | null | undefined): string => (epoch === null || epoch === undefined ? "epoch not given" : `epoch J${epoch}`);

/** Where the object is on the sky: right ascension and declination, and galactic longitude and
 * latitude, each with its frame and epoch, and the entry the position is taken from. */
function Sky({ record, context, title }: Titled & { context: RecordContext }) {
  const sky = record.sky_position;
  if (sky === null || sky === undefined || sky.ra === undefined || sky.dec === undefined) return null;
  const galactic = context.toGalactic === undefined ? undefined : galacticOf(sky.ra, sky.dec, context.toGalactic);
  const from = sky.entry === undefined ? undefined : record.entries[sky.entry];
  const error = sky.uncertainty_mas === null || sky.uncertainty_mas === undefined ? "no uncertainty reported" : `± ${sky.uncertainty_mas.toPrecision(2)} mas`;
  return (
    <Part title="On the sky" testid="sky">
      <ul className="space-y-0.5">
        <li>
          RA {raText(sky.ra)}, Dec {decText(sky.dec)}{" "}
          <span className="text-dim">
            ({sky.frame.toUpperCase()}, {epochText(sky.epoch)}, {error})
          </span>
        </li>
        {galactic === undefined ? null : (
          <li>
            l {galactic.l.toFixed(4)}°, b {galactic.b < 0 ? "−" : "+"}
            {Math.abs(galactic.b).toFixed(4)}° <span className="text-dim">(galactic, from that position, {epochText(sky.epoch)})</span>
          </li>
        )}
      </ul>
      {from === undefined ? null : <p className="text-xs text-dim">From {title(from.source)}</p>}
    </Part>
  );
}

/** The preferred distance, its rule, and every other distance measurement beside it. */
function Distance({ record, title }: Titled) {
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
            <li key={`${m.entry}:${m.quantity}:${m.n}`}>
              <Mark kind={m.kind_of_value} />
              {valueText(m.native)}{" "}
              <span className="text-dim">
                ({words(m.quantity)}, {m.kind_of_value}, {title(m.source)})
              </span>
            </li>
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

/** A measurement's line: its mark, label, value as published, and its kind and source; a
 * redshift also as z where Univ Server normalized it. */
function MeasurementLine({ m, label, title }: { m: Measurement; label: string; title: (id: string) => string }) {
  const z = m.quantity === "redshift" && m.normalized?.unit === "dimensionless" ? `, z = ${m.normalized.value.toPrecision(4)}` : "";
  return (
    <li>
      <Mark kind={m.kind_of_value} />
      {label} {valueText(m.native)}
      {z}{" "}
      <span className="text-dim">
        ({m.kind_of_value}, {title(m.source)})
      </span>
    </li>
  );
}

/** The magnitudes, and the proper motion, radial velocity and redshift, where sources publish
 * them. */
function Values({ record, title }: Titled) {
  const magnitudes = record.measurements.filter((m) => m.quantity === "magnitude");
  const motion = record.measurements.filter((m) => m.quantity in MOTION);
  const line = (m: Measurement, label: string) => <MeasurementLine key={`${m.entry}:${m.quantity}:${m.n}`} m={m} label={label} title={title} />;
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
function Star({ record, title }: Titled) {
  const types = publishedTypes(record);
  const colours = colourIndices(record);
  const estimated = estimatedClass(types, colours);
  if (types.length === 0 && colours.length === 0) return null;
  return (
    <Part title="Spectral type and colour" testid="star">
      <ul className="space-y-0.5">
        {types.map((t) => (
          <li key={`t:${t.source}:${t.text}`}>
            <Mark kind={t.kind} />
            {t.text}{" "}
            <span className="text-dim">
              ({t.kind}, {title(t.source)})
            </span>
          </li>
        ))}
        {estimated === undefined ? null : (
          <li>
            <Mark kind="estimated" />
            Estimated class {estimated}
          </li>
        )}
        {colours.map((m) => (
          <li key={`c:${m.entry}:${m.n}`}>
            <Mark kind={m.kind_of_value} />
            {bandsText(m.band ?? "colour index")} {valueText(m.native)}{" "}
            <span className="text-dim">
              ({m.kind_of_value}, {title(m.source)}
              {m.derived_from === null || m.derived_from === undefined ? "" : `, derived from ${m.derived_from.join(" − ")}`})
            </span>
          </li>
        ))}
      </ul>
    </Part>
  );
}

/** The groups the object belongs to, or the members of the group it is, each to choose. */
function Groups({ record, context, title }: Titled & { context: RecordContext }) {
  const related = relationsByOther(record);
  if (related.length === 0) return null;
  return (
    <Part title="Groups" testid="groups">
      <ul className="space-y-0.5">
        {related.map(({ other, kinds }) => (
          <li key={`${other.source}:${other.identifier}`}>
            {relationText(kinds)}{" "}
            <button
              type="button"
              onClick={() => {
                context.onChooseDesignation(other.source, other.identifier);
              }}
              className="text-accent hover:underline"
            >
              {other.identifier}
            </button>{" "}
            <span className="text-dim">({title(other.source)})</span>
          </li>
        ))}
      </ul>
    </Part>
  );
}

/** The structures of the release whose volume holds the object, each to choose. */
function Inside({ record, context }: { record: ObjectRecord; context: RecordContext }) {
  const place = placeOf(record);
  const holding = place === undefined ? [] : context.holding(place);
  if (holding.length === 0) return null;
  return (
    <Part title="Inside structures" testid="inside">
      <ul className="space-y-0.5">
        {holding.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => {
                context.onChooseStructure(s.id);
              }}
              className="font-serif text-accent italic hover:underline"
            >
              {s.name}
            </button>
          </li>
        ))}
      </ul>
    </Part>
  );
}

/** Each catalogue entry of the object, with its flags and what their values mean. */
function Catalogues({ record, context, title }: Titled & { context: RecordContext }) {
  const items = record.entries.map((e) => {
    const flags = flagsOf(e, context.sources.details.get(e.source));
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
function Papers({ record }: { record: ObjectRecord }) {
  const researched = record.entries.filter((e) => (e.references ?? []).length > 0);
  if (researched.length === 0) return null;
  return (
    <Part title="Papers of the researched values" testid="papers">
      <ul className="space-y-0.5">
        {researched.flatMap((e) =>
          (e.references ?? []).map((r) => (
            <li key={`${e.source}:${r.citation}`}>
              {r.citation} <span className="text-dim">({[r.bibcode, r.doi, e.source].filter((x) => x !== null && x !== undefined).join(", ")})</span>
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
