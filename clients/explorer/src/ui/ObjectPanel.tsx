import type { Found, ObjectRecord } from "../api/objects.ts";
import type { Mat3, Vec3 } from "../scene/vec.ts";
import { RecordBody } from "./RecordSections.tsx";
import { headingOf } from "./text.ts";
import type { ChosenObject } from "./useChosen.ts";
import type { Sources } from "./useSources.ts";

/** What the panel reads besides the record: its release and that release's sources, the
 * rotation to galactic axes, the structures holding a place, and the ways to choose what it
 * names. */
export interface RecordContext {
  release: number;
  /** The release's sources where they are known already; the panel reads them otherwise. */
  sources?: Sources | undefined;
  /** The rotation from the ICRS to galactic axes, once the release's manifests give it. */
  toGalactic: Mat3 | undefined;
  /** The structures of the release whose volume holds `place`, ICRS megaparsecs. */
  holding: (place: Vec3) => { id: string; name: string }[];
  onChooseStructure: (id: string) => void;
  /** Looks up the object that `source` names `identifier`. */
  find: (source: string, identifier: string) => Promise<Found>;
  /** Chooses object `id`, its record given where it is read already. */
  onChooseObject: (id: number, record?: ObjectRecord) => void;
}

const none = () => undefined;

export const NO_CONTEXT: RecordContext = {
  release: 0,
  toGalactic: undefined,
  holding: () => [],
  onChooseStructure: none,
  find: (source, identifier) => Promise.resolve({ kind: "none", query: `${source}:${identifier}` }),
  onChooseObject: none,
};

interface ObjectPanelProps {
  chosen: ChosenObject;
  onChoose: (id: number) => void;
  onClose: () => void;
  context?: RecordContext | undefined;
}

/** The chosen object's record, as a gazetteer gives an entry: its name, then what is known of
 * it, each value with its source and the mark of its kind. */
export function ObjectPanel({ chosen, onChoose, onClose, context = NO_CONTEXT }: ObjectPanelProps) {
  if (chosen.kind === "none") return null;
  return (
    <section aria-label="The chosen object" className="space-y-3 p-3 text-sm">
      <div className="flex items-start justify-between gap-2">
        <h2 className="font-serif text-lg leading-snug">
          {chosen.kind === "ready" ? (
            <>
              {headingOf(chosen.record, chosen.as)} <span className="font-sans text-sm text-dim">#{chosen.record.id}</span>
            </>
          ) : (
            "Object"
          )}
        </h2>
        <button type="button" onClick={onClose} className="text-dim hover:text-ink" aria-label="Close the object's panel">
          ×
        </button>
      </div>
      <Body chosen={chosen} onChoose={onChoose} context={context} />
    </section>
  );
}

function Body({ chosen, onChoose, context }: { chosen: Exclude<ChosenObject, { kind: "none" }>; onChoose: (id: number) => void; context: RecordContext }) {
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
      return <RecordBody record={chosen.record} context={context} />;
  }
}
