import type { ReactNode } from "react";

import { Swatch } from "./Swatch.tsx";
import { SwitchBox } from "./SwitchBox.tsx";
import type { Switch } from "./addressSwitch.ts";
import type { Catalogue } from "../catalogue/catalogue.ts";
import { AXES, type Axes } from "../scene/frames.ts";
import { formatDistance, shortDistance } from "../scene/format.ts";
import { KINDS, POINT_CSS } from "../scene/kinds.ts";
import { Mark } from "./Mark.tsx";
import { GREY_CSS, STAR_GRADIENT } from "../scene/starColour.ts";
import { StarScale } from "./record/charts.tsx";
import { DWARF_SEQUENCE_CITATION } from "../scene/dwarfSequence.ts";
import type { Controller } from "../scene/controller.ts";
import { FLOATING } from "./floating.ts";
import { type Store, useStore } from "./store.ts";

/** What the panels read of the view: its controller, redrawn with each frame drawn. */
interface ViewProps {
  controller: Controller;
  /** Counts the frames drawn: the panels show the view as each leaves it. */
  frame: Store<number>;
}

/** What the view's drawer says of the rings drawn in the axes' equator. */
function ringsText(controller: Controller): string {
  if (!controller.ringsOn) return `Ring: the ${controller.axes} equator, ${formatDistance(controller.ringRadius)} from Earth.`;
  const { rings } = controller;
  return rings === undefined
    ? `Rings: none within reach of the focus in the ${controller.axes} equator.`
    : `Rings: every ${shortDistance(rings.step)} from the Solar System barycentre, in the ${controller.axes} equator.`;
}

/** The top bar's part: the eye's distance from the focus, and the axes with the grid's switch. */
export function Whereabouts({ controller, frame, grid, rings }: ViewProps & { grid: Switch; rings: Switch }) {
  useStore(frame);
  return (
    <div className="flex shrink-0 items-center gap-3 text-sm">
      <p data-testid="distance" className="whitespace-nowrap">
        <span className="text-dim max-xl:sr-only">Eye to focus </span>
        {formatDistance(controller.view.distance)}
      </p>
      <label className="flex items-center gap-1.5">
        <span className="text-dim">Axes</span>
        <select
          value={controller.axes}
          onChange={(event) => {
            controller.setAxes(event.target.value as Axes);
          }}
          className="rounded border border-line bg-space px-1.5 py-0.5"
        >
          {AXES.map((axes) => (
            <option key={axes} value={axes} disabled={!controller.canShow(axes)}>
              {axes}
            </option>
          ))}
        </select>
      </label>
      <SwitchBox label="Grid" state={grid} textClass="text-dim" />
      <SwitchBox label="Rings" state={rings} textClass="text-dim" />
    </div>
  );
}

/** The view's drawer: the ring, the frame of the axes, and how the mouse moves the view. */
export function ViewDrawer({ controller, frame, cosmology }: ViewProps & { cosmology: string }) {
  useStore(frame);
  return (
    <div className="space-y-3">
      <h2 className="font-serif text-base">The view</h2>
      <p>
        {ringsText(controller)}
      </p>
      <p className="text-dim" data-testid="frame">
        Axes {controller.axes} ({controller.frame.basis}). Origin: the Solar System barycentre, within about 1 AU of Earth.
        Distances in parsecs to gigaparsecs, positions in megaparsecs; cosmology {cosmology}.
      </p>
      <p className="text-dim">
        The edges give the direction of sight through them in the chosen axes, the same from wherever the eye is; the scale
        below holds at the distance of the focus.
      </p>
    </div>
  );
}

/** What the key reads of the catalogue. */
export type Tally = Pick<Catalogue, "kinds" | "stars" | "failed" | "unread" | "credits">;

/** A part of the bottom bar that opens upward over the view. */
function Popover({ label, children, width = "w-96", side = "left-0" }: { label: string; children: ReactNode; width?: string; side?: string }) {
  return (
    <details className="relative shrink-0">
      <summary className="cursor-pointer list-none rounded px-1.5 py-0.5 hover:text-accent [&::-webkit-details-marker]:hidden">{label}</summary>
      <div
        className={`${FLOATING} bottom-full mb-2 max-h-[60vh] max-w-[calc(100vw-1.5rem)] overflow-y-auto whitespace-normal ${side} ${width}`}
      >
        {children}
      </div>
    </details>
  );
}

/** The bottom bar's key and credits: the stars' colours and the count of each kind of value in
 * one line, each opening into the full key, and the credits of the drawn points' sources. */
export function KeyBar({ catalogue: tally, frame, release }: { catalogue: () => Tally | undefined; frame: Store<number>; release: number }) {
  useStore(frame);
  const catalogue = tally();
  if (catalogue === undefined) return null;
  const kinds = KINDS.map((kind, i) => [kind, catalogue.kinds[i] ?? 0] as const);
  const stars = catalogue.stars.coloured + catalogue.stars.grey;
  const missing = catalogue.failed + catalogue.unread;
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <div className="hidden min-w-0 items-center gap-3 overflow-hidden whitespace-nowrap xl:flex">
        <span className="flex items-center gap-1.5">
          <span className="inline-block h-2 w-12 rounded-sm" style={{ background: STAR_GRADIENT }} />
          {stars.toLocaleString("en-GB")} stars
        </span>
        {kinds.map(([kind, count]) => (
          <span key={kind} className="flex items-center">
            <Mark kind={kind} />
            {kind} {count.toLocaleString("en-GB")}
          </span>
        ))}
      </div>
      <Popover label="Key">
        <Key catalogue={catalogue} />
      </Popover>
      {missing > 0 ? <span className="shrink-0 text-warn">{missing.toLocaleString("en-GB")} tiles not drawn</span> : null}
      <Popover label="Credits" width="w-[28rem]" side="right-0">
        <h2 className="font-semibold">The sources of release {release}'s points</h2>
        <ul className="mt-2 space-y-2" data-testid="credits">
          {catalogue.credits().map((source) => (
            <li key={`${source.domain}/${source.id}`}>
              <p className="font-medium">{source.title}</p>
              <p className="text-xs text-dim">{source.citation}</p>
              <p className="text-xs">{source.credit}</p>
            </li>
          ))}
        </ul>
      </Popover>
    </div>
  );
}

function Key({ catalogue }: { catalogue: Tally }) {
  const kinds = KINDS.map((kind, i) => [kind, catalogue.kinds[i] ?? 0] as const);
  return (
    <div className="space-y-2">
      <h2 className="font-semibold">Stars, by colour index</h2>
      <div data-testid="star-key" className="space-y-1">
        <StarScale />
        <p className="text-xs">
          {catalogue.stars.coloured.toLocaleString("en-GB")} stars in the colour of the temperature their colour index gives,
          estimated along the dwarf sequence of {DWARF_SEQUENCE_CITATION}, as observed and not corrected for reddening. The
          sequence's BP−RP stops at B9V, so a hotter star coloured by BP−RP is drawn at about 10,700 K.
        </p>
        <p className="flex items-center gap-2 text-xs">
          <Swatch colour={GREY_CSS} />
          {catalogue.stars.grey.toLocaleString("en-GB")} stars in grey, with no colour index.
        </p>
      </div>
      <h2 className="flex items-center gap-2 font-semibold">
        <Swatch colour={POINT_CSS} />
        Other points, by the kind of value of each distance
      </h2>
      <ul data-testid="key">
        {kinds.map(([kind, count]) => (
          <li key={kind}>
            <Mark kind={kind} />
            {kind}: {count.toLocaleString("en-GB")} points
          </li>
        ))}
      </ul>
      {catalogue.failed > 0 ? (
        <p role="status" className="text-warn">
          {catalogue.failed} tiles failed their check and are not drawn.
        </p>
      ) : null}
      {catalogue.unread > 0 ? (
        <p role="status" className="text-warn">
          {catalogue.unread} tiles could not be read from Univ Server and are not drawn.
        </p>
      ) : null}
    </div>
  );
}
