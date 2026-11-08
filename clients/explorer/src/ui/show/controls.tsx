// The small controls the Show drawer is made of (choose-what-is-drawn): a toggle chip, the link
// to show only one, and a part's heading with All and None.

import type { ReactNode } from "react";

/** A toggle drawn as a chip: filled while on, hollow and dashed while off, and pressed or not for
 * assistive technology. */
export function Chip({ on, name, onPress, children }: { on: boolean; name: string; onPress: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      aria-label={name}
      onClick={onPress}
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-px text-xs ${
        on ? "border-[#3a4a6e] bg-[#16213a]" : "border-dashed border-[#2e3a57] text-dim"
      }`}
    >
      {children}
    </button>
  );
}

/** The link that turns on only the group of its line, taking no room until the pointer is over
 * the line or it has the focus, and always reachable by the keyboard. */
export function Only({ what, onOnly }: { what: string; onOnly: () => void }) {
  return (
    <button
      type="button"
      aria-label={`Only ${what}`}
      onClick={onOnly}
      className="sr-only text-xs text-accent group-focus-within:not-sr-only group-hover:not-sr-only focus:not-sr-only"
    >
      only
    </button>
  );
}

/** A part's heading, with its All and None. */
export function PartHeading({ title, onAll, onNone }: { title: string; onAll: () => void; onNone: () => void }) {
  return (
    <div className="flex items-baseline justify-between">
      <h3 className="font-serif text-[15px] font-semibold">{title}</h3>
      <span className="text-xs text-dim">
        <button type="button" aria-label={`All ${title.toLowerCase()}`} onClick={onAll} className="text-accent hover:underline">
          All
        </button>
        {" · "}
        <button type="button" aria-label={`No ${title.toLowerCase()}`} onClick={onNone} className="text-accent hover:underline">
          None
        </button>
      </span>
    </div>
  );
}

/** A dim line naming what follows it. */
export const Caption = ({ children }: { children: ReactNode }) => <p className="mt-1 text-xs text-dim">{children}</p>;
