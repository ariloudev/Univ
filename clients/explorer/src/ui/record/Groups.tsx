// The group an object belongs to, or the members of the group it is, each to choose
// (read-an-object): a name that leads to several objects offers them, and one that leads to none
// says so.

import { useState } from "react";

import type { Found, ObjectRecord } from "../../api/objects.ts";
import type { RecordContext } from "../ObjectPanel.tsx";
import { Folded } from "../RecordSections.tsx";
import { formedAs, relationText, relationsByOther } from "../recordText.ts";
import type { Sources } from "../useSources.ts";
import { Chooser, Section } from "./parts.tsx";

/** What looking up a related object answered, where it chose none: the objects it leads to, or
 * why it led to none. */
type Answer = { key: string } & ({ kind: "choices"; ids: number[] } | { kind: "none" } | { kind: "failed"; message: string });

interface GroupsProps {
  record: ObjectRecord;
  context: RecordContext;
  sources: Sources;
  /** The section's heading: its members, or the groups it belongs to. */
  title: string;
  /** Names a source as the panel names it. */
  name: (source: string) => string;
}

/** Each object the record relates this one to, in its catalogue's form, to choose. */
export function Groups({ record, context, sources, title, name }: GroupsProps) {
  const [answer, setAnswer] = useState<Answer>();
  const related = relationsByOther(record);
  if (related.length === 0) return null;
  const choose = (source: string, identifier: string) => {
    const key = `${source}:${identifier}`;
    context
      .find(source, identifier)
      .then((found: Found) => {
        if (found.kind === "object") context.onChooseObject(found.record.id, found.record);
        else setAnswer(found.kind === "choices" ? { key, kind: "choices", ids: found.objects.map((o) => o.id) } : { key, kind: "none" });
      })
      .catch((error: unknown) => {
        setAnswer({ key, kind: "failed", message: error instanceof Error ? error.message : String(error) });
      });
  };
  const items = related.map(({ other, kinds }) => {
    const key = `${other.source}:${other.identifier}`;
    return (
      <li key={key}>
        {relationText(kinds)}{" "}
        <Chooser
          onClick={() => {
            choose(other.source, other.identifier);
          }}
        >
          {formedAs(other.identifier, sources.details.get(other.source))}
        </Chooser>{" "}
        <span className="text-dim">({name(other.source)})</span>
        {answer?.key === key ? <AnswerLine answer={answer} onChoose={(id) => context.onChooseObject(id)} /> : null}
      </li>
    );
  });
  return (
    <Section title={title} testid="groups">
      <Folded items={items} />
    </Section>
  );
}

function AnswerLine({ answer, onChoose }: { answer: Answer; onChoose: (id: number) => void }) {
  switch (answer.kind) {
    case "choices":
      return (
        <p className="text-xs">
          It names {answer.ids.length} objects:{" "}
          {answer.ids.map((id) => (
            <span key={id} className="mr-2">
              <Chooser
                onClick={() => {
                  onChoose(id);
                }}
              >
                #{id}
              </Chooser>
            </span>
          ))}
        </p>
      );
    case "none":
      return <p className="text-xs text-warn">No object of this release carries it.</p>;
    case "failed":
      return (
        <p role="alert" className="text-xs text-warn">
          It could not be read: {answer.message}
        </p>
      );
  }
}
