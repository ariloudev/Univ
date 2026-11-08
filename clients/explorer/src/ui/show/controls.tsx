// The small controls the Show drawer is made of (choose-what-is-drawn): a group's toggle chip,
// the link to show only one, and a part's heading with All and None.

import type { ReactNode } from "react";

import type { Apply, Group, Hidden } from "../hidden.ts";
import { Chooser, Section } from "../record/parts.tsx";

/** A group's toggle drawn as a chip: filled while it is on, hollow and dashed while off, and
 * pressed or not for assistive technology. */
export function GroupChip({ group, name, label, hidden, onApply, children }: { group: Group; name: string; label: string; hidden: Hidden; onApply: Apply; children: (on: boolean) => ReactNode }) {
  const on = !hidden[group].has(name);
  return (
    <button
      type="button"
      aria-pressed={on}
      aria-label={label}
      onClick={() => onApply([{ group, name, shown: !on }])}
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-px text-xs ${on ? "border-edge bg-raised" : "border-dashed border-edge text-dim"}`}
    >
      {children(on)}
    </button>
  );
}

/** Where the link to show only one is set, `over` the end of its line or `beside` it, so that
 * showing it moves nothing. */
const ONLY_PLACES = { over: "absolute inset-y-0 right-0 bg-panel pl-1.5", beside: "absolute left-full z-10 ml-0.5 rounded bg-panel px-1" };

/** The link that turns on only the group of its line: unseen until the pointer is over the line
 * or it has the focus, and always reachable by the keyboard. */
export function Only({ what, onOnly, place }: { what: string; onOnly: () => void; place: keyof typeof ONLY_PLACES }) {
  return (
    <button
      type="button"
      aria-label={`Only ${what}`}
      onClick={onOnly}
      className={`text-xs text-accent opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 focus:opacity-100 ${ONLY_PLACES[place]}`}
    >
      only
    </button>
  );
}

/** A part of the drawer: its heading with its All and None, and what it holds. */
export function Part({ title, onAll, onNone, children }: { title: string; onAll: () => void; onNone: () => void; children: ReactNode }) {
  const what = title.toLowerCase();
  return (
    <Section
      title={title}
      aside={
        <span className="text-xs text-dim">
          <Chooser onClick={onAll} label={`All ${what}`}>
            All
          </Chooser>
          {" · "}
          <Chooser onClick={onNone} label={`No ${what}`}>
            None
          </Chooser>
        </span>
      }
    >
      {children}
    </Section>
  );
}
