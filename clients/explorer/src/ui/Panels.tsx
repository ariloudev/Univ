import { Swatch } from "./Swatch.tsx";
import type { Catalogue } from "../catalogue/catalogue.ts";
import { AXES, type Axes } from "../scene/frames.ts";
import { formatDistance } from "../scene/format.ts";
import { KINDS, POINT_CSS } from "../scene/kinds.ts";
import { GREY_CSS, KEY_TEMPERATURES, STAR_GRADIENT } from "../scene/starColour.ts";
import { DWARF_SEQUENCE_CITATION } from "../scene/dwarfSequence.ts";
import type { Controller } from "../scene/controller.ts";
import { type Store, useStore } from "./store.ts";

interface PanelsProps {
  controller: Controller;
  /** The catalogue, once its domains are read. */
  catalogue: () => Tally | undefined;
  /** Counts the frames drawn: the panels show the view as each leaves it. */
  frame: Store<number>;
  release: number;
  cosmology: string;
}

/** The panels around the view: where the eye is, the axes, the key and the credits. */
export function Panels({ controller, catalogue: tally, frame, release, cosmology }: PanelsProps) {
  useStore(frame);
  const catalogue = tally();
  return (
    <>
      <div className="w-80 space-y-2 rounded-lg border border-line bg-panel p-3 text-sm">
        <p data-testid="distance">
          <span className="text-dim">Eye to focus: </span>
          {formatDistance(controller.view.distance)}
        </p>
        <p className="text-dim">
          Ring: the {controller.axes} equator, {formatDistance(controller.ringRadius)} from Earth
        </p>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2">
            <span className="text-dim">Axes</span>
            <select
              value={controller.axes}
              onChange={(event) => {
                controller.setAxes(event.target.value as Axes);
              }}
              className="rounded border border-line bg-space px-2 py-1"
            >
              {AXES.map((axes) => (
                <option key={axes} value={axes} disabled={!controller.canShow(axes)}>
                  {axes}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            onClick={() => {
              controller.returnToEarth();
            }}
            className="rounded border border-accent px-2 py-1 text-accent hover:bg-accent hover:text-space"
          >
            Return to Earth
          </button>
        </div>
        <FrameStatement axes={controller.axes} basis={controller.frame.basis} cosmology={cosmology} />
      </div>
      {catalogue === undefined ? null : <Key catalogue={catalogue} release={release} />}
    </>
  );
}

function FrameStatement({ axes, basis, cosmology }: { axes: Axes; basis: string; cosmology: string }) {
  return (
    <p className="text-xs text-dim" data-testid="frame">
      Axes {axes} ({basis}). Origin: the Solar System barycentre, within about 1 AU of Earth.
      Distances in parsecs to gigaparsecs, positions in megaparsecs; cosmology {cosmology}.
    </p>
  );
}

/** What the key reads of the catalogue. */
export type Tally = Pick<Catalogue, "kinds" | "stars" | "failed" | "unread" | "credits">;

function Key({ catalogue, release }: { catalogue: Tally; release: number }) {
  const kinds = KINDS.map((kind, i) => [kind, catalogue.kinds[i] ?? 0] as const);
  const credits = catalogue.credits();
  return (
    <div className="fixed right-3 bottom-3 max-h-[35%] w-96 space-y-2 overflow-y-auto rounded-lg border border-line bg-panel p-3 text-sm">
      <h2 className="font-semibold">Stars, by colour index</h2>
      <div data-testid="star-key" className="space-y-1">
        <div className="h-3 rounded" style={{ background: STAR_GRADIENT }} />
        <div className="flex justify-between text-xs text-dim">
          {KEY_TEMPERATURES.map((t) => (
            <span key={t}>{t.toLocaleString("en-GB")} K</span>
          ))}
        </div>
        <p className="text-xs">
          {catalogue.stars.coloured.toLocaleString("en-GB")} stars in the colour of the temperature their colour
          index gives, estimated along the dwarf sequence of {DWARF_SEQUENCE_CITATION}, as observed and not
          corrected for reddening. The sequence's BP−RP stops at B9V, so a hotter star coloured by BP−RP is drawn
          at about 10,700 K.
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
      <details>
        <summary className="cursor-pointer">Credits: the sources of release {release}'s points</summary>
        <ul className="mt-2 space-y-2" data-testid="credits">
          {credits.map((source) => (
            <li key={`${source.domain}/${source.id}`}>
              <p className="font-medium">{source.title}</p>
              <p className="text-xs text-dim">{source.citation}</p>
              <p className="text-xs">{source.credit}</p>
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}
