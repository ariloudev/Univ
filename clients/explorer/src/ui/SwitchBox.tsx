import type { Switch } from "./addressSwitch.ts";

/** A switch in the top bar: a checkbox with the role of a switch, named by `label`, whose text is
 * set by `textClass`. */
export function SwitchBox({ label, state: [on, set], textClass }: { label: string; state: Switch; textClass: string }) {
  return (
    <label className="flex items-center gap-1.5">
      <input
        type="checkbox"
        role="switch"
        checked={on}
        onChange={(event) => {
          set(event.currentTarget.checked);
        }}
        className="accent-accent"
      />
      <span className={textClass}>{label}</span>
    </label>
  );
}
