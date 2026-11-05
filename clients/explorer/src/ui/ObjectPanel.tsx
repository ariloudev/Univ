import { type ObjectRecord, distancesOf } from "../api/objects.ts";
import type { ChosenObject } from "./useChosen.ts";
import { colourIndices, estimatedClass, publishedTypes } from "./starText.ts";
import { bandsText, headingOf, natureOf, preferredText, valueText, words } from "./text.ts";

/** The chosen object's record: its names, its preferred and other distances, its frame. */
export function ObjectPanel({ chosen, onChoose, onClose }: { chosen: ChosenObject; onChoose: (id: number) => void; onClose: () => void }) {
  if (chosen.kind === "none") return null;
  return (
    <section aria-label="The chosen object" className="space-y-2 rounded-lg border border-line bg-panel p-3 text-sm">
      <div className="flex items-start justify-between gap-2">
        <h2 className="font-semibold">
          {chosen.kind === "ready" ? (
            <>
              {headingOf(chosen.record, chosen.as)} <span className="font-normal text-dim">#{chosen.record.id}</span>
            </>
          ) : (
            "Object"
          )}
        </h2>
        <button type="button" onClick={onClose} className="text-dim hover:text-ink" aria-label="Close the object's panel">
          ×
        </button>
      </div>
      <Body chosen={chosen} onChoose={onChoose} />
    </section>
  );
}

function Body({ chosen, onChoose }: { chosen: Exclude<ChosenObject, { kind: "none" }>; onChoose: (id: number) => void }) {
  switch (chosen.kind) {
    case "choices":
      return (
        <div>
          <p>Object {chosen.choices.query} was replaced by {chosen.choices.objects.length} objects:</p>
          <ul>
            {chosen.choices.objects.map((o) => (
              <li key={o.id}>
                <button type="button" onClick={() => onChoose(o.id)} className="text-accent hover:underline">
                  {o.id}
                </button>
              </li>
            ))}
          </ul>
        </div>
      );
    case "loading":
      return <p role="status" className="text-dim">Reading object {chosen.id}…</p>;
    case "failed":
      return (
        <p role="alert" className="text-warn">
          Univ Server answered {chosen.status ?? "nothing"}: {chosen.title}
        </p>
      );
    case "ready":
      return <Record record={chosen.record} />;
  }
}

function Record({ record }: { record: ObjectRecord }) {
  const { preferred, others } = distancesOf(record);
  const names = record.entries.filter((e) => e.name !== null);
  return (
    <div className="space-y-2">
      <p className="text-dim" data-testid="nature">
        {natureOf(record)}
      </p>
      <div>
        <h3 className="text-xs text-dim">Names and designations</h3>
        <ul data-testid="names">
          {names.map((e) => (
            <li key={`n:${e.source}:${e.identifier}`}>
              {e.name} <span className="text-dim">({e.source})</span>
            </li>
          ))}
          {record.designations.map((d) => (
            <li key={`d:${d.source}:${d.identifier}`}>
              {d.identifier} <span className="text-dim">({d.source})</span>
            </li>
          ))}
        </ul>
      </div>
      <Preferred record={record} source={preferred?.source} kind={preferred?.kind_of_value} />
      {others.length === 0 ? null : (
        <div>
          <h3 className="text-xs text-dim">Other distance measurements</h3>
          <ul data-testid="other-distances">
            {others.map((m) => (
              <li key={`${m.entry}:${m.quantity}:${m.n}`}>
                {valueText(m.native)} <span className="text-dim">({words(m.quantity)}, {m.kind_of_value}, {m.source})</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <Star record={record} />
      <Papers record={record} />
      <Frame record={record} />
    </div>
  );
}

/** A star's published spectral types, its colour indices, and the class its colour index
 * suggests where no source publishes a type. */
function Star({ record }: { record: ObjectRecord }) {
  const types = publishedTypes(record);
  const colours = colourIndices(record);
  const estimated = estimatedClass(types, colours);
  if (types.length === 0 && colours.length === 0) return null;
  return (
    <div data-testid="star">
      <h3 className="text-xs text-dim">Spectral type and colour</h3>
      <ul>
        {types.map((t) => (
          <li key={`t:${t.source}:${t.text}`}>
            {t.text} <span className="text-dim">({t.kind}, {t.source})</span>
          </li>
        ))}
        {estimated === undefined ? null : (
          <li>
            Estimated class {estimated}
          </li>
        )}
        {colours.map((m) => (
          <li key={`c:${m.entry}:${m.n}`}>
            {bandsText(m.band ?? "colour index")} {valueText(m.native)}{" "}
            <span className="text-dim">
              ({m.kind_of_value}, {m.source}
              {m.derived_from === null || m.derived_from === undefined ? "" : `, derived from ${m.derived_from.join(" − ")}`})
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The papers each researched entry's values come from. */
function Papers({ record }: { record: ObjectRecord }) {
  const researched = record.entries.filter((e) => (e.references ?? []).length > 0);
  if (researched.length === 0) return null;
  return (
    <div data-testid="papers">
      <h3 className="text-xs text-dim">Papers of the researched values</h3>
      <ul>
        {researched.flatMap((e) =>
          (e.references ?? []).map((r) => (
            <li key={`${e.source}:${r.citation}`}>
              {r.citation} <span className="text-dim">({[r.bibcode, r.doi, e.source].filter((x) => x !== null && x !== undefined).join(", ")})</span>
            </li>
          )),
        )}
      </ul>
    </div>
  );
}

function Preferred({ record, source, kind }: { record: ObjectRecord; source: string | undefined; kind: string | undefined }) {
  const preferred = record.preferred;
  if (preferred === null) return <p className="text-dim">No distance: the object has a sky position only.</p>;
  return (
    <div data-testid="preferred">
      <h3 className="text-xs text-dim">Preferred distance</h3>
      <p>{preferredText(preferred)}</p>
      <p className="text-xs text-dim">
        {kind ?? "unknown kind"} · {source ?? "unknown source"} · chosen by the rule “{words(preferred.rule)}”
      </p>
    </div>
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
