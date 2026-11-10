import type { ReactNode } from "react";

/** The drawers the rail opens. */
export type DrawerName = "view" | "show" | "structures";

const ICON = { width: 20, height: 20, viewBox: "0 0 20 20", fill: "none", stroke: "currentColor", strokeWidth: 1.4, "aria-hidden": true } as const;

/** Each drawer: its rail button's label, its own title, and its icon. */
const DRAWERS: readonly { name: DrawerName; label: string; title: string; icon: ReactNode }[] = [
  {
    name: "view",
    label: "View",
    title: "The view",
    icon: (
      <svg {...ICON}>
        <circle cx="10" cy="10" r="6.5" />
        <path d="M10 1.5v4M10 14.5v4M1.5 10h4M14.5 10h4" />
      </svg>
    ),
  },
  {
    name: "show",
    label: "Show",
    title: "Show",
    icon: (
      <svg {...ICON}>
        <path d="M10 3 18 7 10 11 2 7Z" />
        <path d="M2 10.5 10 14.5 18 10.5M2 14 10 18 18 14" />
      </svg>
    ),
  },
  {
    name: "structures",
    label: "Structures",
    title: "Structures",
    icon: (
      <svg {...ICON}>
        <path d="M4 6 9 3 16 5 17 12 12 17 5 15 3 10Z" strokeDasharray="2.2 1.6" />
        <circle cx="10" cy="10" r="1.2" fill="currentColor" />
      </svg>
    ),
  },
];

/** Whether `value` names a drawer the rail opens. */
export const isDrawerName = (value: unknown): value is DrawerName => DRAWERS.some((d) => d.name === value);

/** The title of drawer `name`. */
export const drawerTitle = (name: DrawerName): string => DRAWERS.find((d) => d.name === name)?.title ?? name;

const BUTTON = "flex w-11 flex-col items-center gap-0.5 rounded py-1.5 text-[10.5px] leading-none hover:text-accent";

/** The rail along the view's left edge: a button for each drawer, which opens it and closes it
 * again, and the button that returns to Earth. */
export function Rail({ open, onToggle, onEarth }: { open: DrawerName | undefined; onToggle: (drawer: DrawerName) => void; onEarth: () => void }) {
  return (
    <nav aria-label="Panels" className="flex w-[52px] shrink-0 flex-col items-center gap-1 border-r border-line bg-panel py-2">
      {DRAWERS.map((drawer) => (
        <button
          key={drawer.name}
          type="button"
          aria-expanded={open === drawer.name}
          aria-controls={`drawer-${drawer.name}`}
          onClick={() => {
            onToggle(drawer.name);
          }}
          className={`${BUTTON} ${open === drawer.name ? "bg-space text-accent" : "text-ink"}`}
        >
          {drawer.icon}
          {drawer.label}
        </button>
      ))}
      <button type="button" onClick={onEarth} aria-label="Return to Earth" title="Return to Earth" className={`${BUTTON} mt-auto text-ink`}>
        <svg {...ICON}>
          <circle cx="10" cy="10" r="7" />
          <path d="M10 3v14M3 10h14" />
        </svg>
        Earth
      </button>
    </nav>
  );
}
