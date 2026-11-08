// The parts every record panel is made of (read-an-object, see-the-structures): a section under
// its heading, lines of a label, a value with its source and the mark of its kind, a lead figure,
// a note, a dispute, and a name that chooses what it names.

import type { ReactNode } from "react";

import { Mark } from "../Mark.tsx";

/** A part of the record under its heading. */
export function Section({ title, testid, children }: { title: string; testid: string; children: ReactNode }) {
  return (
    <section className="space-y-1.5" data-testid={testid}>
      <h3 className="font-serif text-[15px] font-semibold">{title}</h3>
      {children}
    </section>
  );
}

/** One line: what it is, the value with the short name of its source, and the mark of its kind. */
export interface Line {
  label: string;
  value: ReactNode;
  source?: string | undefined;
  kind?: string | undefined;
}

/** Lines of label, value and kind in three columns. */
export function Rows({ lines }: { lines: readonly Line[] }) {
  return (
    <div className="grid grid-cols-[minmax(76px,max-content)_minmax(0,1fr)_14px] items-baseline gap-x-3 gap-y-0.5" data-testid="rows">
      {lines.map((l, i) => (
        <Row key={`${l.label}:${i}`} line={l} />
      ))}
    </div>
  );
}

function Row({ line }: { line: Line }) {
  return (
    <>
      <span className="max-w-[150px] text-dim">{line.label}</span>
      <span>
        {line.value}
        {line.source === undefined ? null : <span className="text-dim"> {line.source}</span>}
      </span>
      <span className="text-right">
        <Mark kind={line.kind} />
      </span>
    </>
  );
}

/** A section's lead: its figure large, the rest beside it, the mark of its kind at the end. */
export function Lead({ figure, children, kind }: { figure: string; children?: ReactNode; kind: string | undefined }) {
  return (
    <p className="flex items-baseline justify-between gap-2" data-testid="lead">
      <span>
        <span className="text-[22px] font-medium">{figure}</span> {children}
      </span>
      <Mark kind={kind} />
    </p>
  );
}

/** An uncertainty set small and raised beside its value. */
export const Raised = ({ children }: { children: ReactNode }) => <span className="align-[4px] text-[11px] text-dim">{children}</span>;

/** A small note beneath a section's lines: its frame, epoch and provenance. */
export function Note({ children, testid }: { children: ReactNode; testid?: string }) {
  return (
    <p className="text-xs text-dim" data-testid={testid}>
      {children}
    </p>
  );
}

/** That what the panel shows is disputed, and by which papers. */
export function Disputed({ by }: { by: readonly string[] }) {
  return (
    <p className="border-l-2 border-warn pl-2.5 text-[13px]" data-testid="disputed">
      <span className="font-semibold text-warn">Disputed</span> by {by.join(", ")}.
    </p>
  );
}

/** A name that chooses what it names. */
export function Chooser({ onClick, italic = false, children }: { onClick: () => void; italic?: boolean; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className={`text-accent hover:underline ${italic ? "font-serif italic" : ""}`}>
      {children}
    </button>
  );
}
