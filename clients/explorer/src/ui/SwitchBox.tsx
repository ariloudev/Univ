import type { Switch } from "./addressSwitch.ts";

/** A checkbox in the top bar, named by `label`, whose text is set by `textClass`: with the role of
 * a switch, as the view's settings are, unless `role` makes it a plain checkbox. */
export function SwitchBox({ label, state: [on, set], textClass, role = "switch" }: { label: string; state: Switch; textClass: string; role?: "switch" | "checkbox" }) {
  return (
    <label className="flex items-center gap-1.5" title={label}>
      <input
        type="checkbox"
        role={role === "switch" ? "switch" : undefined}
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
