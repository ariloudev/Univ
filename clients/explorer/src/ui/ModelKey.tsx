// The Galaxy model in the key and the credits (see-the-galaxy-model): its points named as a
// model, not observed stars, and counted apart from the objects drawn; and the papers its
// definition cites.

import type { ModelLook } from "../catalogue/tiles.ts";
import { Swatch } from "./Swatch.tsx";

/** The Galaxy model in the key, while its domain is drawn: each population in its colour, and
 * its points drawn. */
export function ModelKey({ model, points }: { model: ModelLook | undefined; points: number }) {
  if (model === undefined) return null;
  return (
    <div data-testid="model-key" className="space-y-1">
      <h2 className="font-semibold">The Galaxy model: a model, not observed stars</h2>
      <p className="text-xs">
        {points.toLocaleString("en-GB")} model points drawn as light, each standing for an equal share of the Galaxy's light,
        not for a star. Where real stars or young tracers are drawn, the model points they cover are not.
      </p>
      <ul className="space-y-1 text-xs">
        {model.populations.map((p) => (
          <li key={p.code} className="flex items-center gap-2">
            <Swatch colour={`rgb(${p.colour.srgb.join(" ")})`} />
            {p.name}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The papers the Galaxy model's definition cites, in the credits while its domain is drawn. */
export function ModelCredits({ model }: { model: ModelLook | undefined }) {
  if (model === undefined) return null;
  return (
    <>
      <h2 className="mt-3 font-semibold">The papers of the Galaxy model</h2>
      <ul className="mt-2 space-y-1 text-xs" data-testid="model-credits">
        {model.papers.map((paper) => (
          <li key={paper.id}>
            {paper.reference}
            {paper.doi === undefined || paper.doi === null ? null : <span className="text-dim"> doi:{paper.doi}</span>}
          </li>
        ))}
      </ul>
    </>
  );
}
