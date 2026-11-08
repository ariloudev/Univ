import type { Switch } from "./addressSwitch.ts";

/** A switch in the top bar: a checkbox with the role of a switch, or a plain checkbox where
 * `checkbox`, named by `label`, whose text is set by `textClass`. */
export function SwitchBox({ label, state: [on, set], textClass, checkbox = false }: { label: string; state: Switch; textClass: string; checkbox?: boolean }) {
  return (
    <label className="flex items-center gap-1.5" title={label}>
      <input
        type="checkbox"
        role={checkbox ? undefined : "switch"}
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
