// The base of every record panel (read-an-object, see-the-structures): the name it is headed by,
// a line of its other names, a sentence of what it is, its sections, and one link that opens the
// whole record, made only once it is opened.

import { type ReactNode, useState } from "react";

export interface RecordPanelProps {
  /** The panel's accessible name, as "The chosen object". */
  label: string;
  title: ReactNode;
  /** Whether the title is set in italic, as a structure is named. */
  italic?: boolean;
  /** Its other names and its id or kind, on one line. */
  subtitle?: ReactNode;
  /** One sentence of what it is. */
  lede?: ReactNode;
  /** The close button's accessible name. */
  closeLabel: string;
  onClose: () => void;
  /** The link that opens the whole record, and the record it opens. */
  fullLabel: string;
  full: () => ReactNode;
  children: ReactNode;
}

/** A record: its heading, sentence and sections, and the whole record under one link. */
export function RecordPanel({ label, title, italic = false, subtitle, lede, closeLabel, onClose, fullLabel, full, children }: RecordPanelProps) {
  return (
    <section aria-label={label} className="space-y-4 px-5 pt-4 pb-6 text-sm">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h2 className={`font-serif text-[26px] leading-tight [overflow-wrap:anywhere] ${italic ? "font-medium italic" : "font-semibold"}`}>{title}</h2>
          {subtitle === undefined ? null : <p className="mt-1 text-dim">{subtitle}</p>}
        </div>
        <button type="button" onClick={onClose} aria-label={closeLabel} className="h-7 w-7 shrink-0 rounded border border-line text-dim hover:text-ink">
          ×
        </button>
      </div>
      {lede === undefined ? null : (
        <p className="text-[15px]" data-testid="lede">
          {lede}
        </p>
      )}
      {children}
      <Full label={fullLabel} full={full} />
    </section>
  );
}

/** The whole record under its link, made only once the link opens it. */
function Full({ label, full }: { label: string; full: () => ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <details data-testid="full-record" open={open}>
      <summary
        className="cursor-pointer text-[13px] text-accent underline underline-offset-4"
        onClick={(event) => {
          event.preventDefault();
          setOpen(!open);
        }}
      >
        {label}
      </summary>
      {open ? <div className="mt-3 space-y-3">{full()}</div> : null}
    </details>
  );
}
