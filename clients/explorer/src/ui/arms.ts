// The arms view's groups and address (see-the-galaxys-arms-face-on): what it draws, the Galaxy's
// young tracers and its model arms, and `view=arms` in the page's address until the person moves
// the view.

import { useCallback, useEffect, useRef } from "react";

import { STRUCTURE_FAMILIES } from "../scene/kinds.ts";
import { TRACERS_DOMAIN, TRACER_KINDS, type TracerKind } from "../scene/tracers.ts";
import { setInAddress } from "./address.ts";
import type { AgeCut } from "./age.ts";
import type { Change, Hidden } from "./hidden.ts";
import { every } from "./show/actions.ts";
import type { ShowPanelProps } from "./show/ShowPanel.tsx";
import type { World } from "./useWorld.ts";

/** The age below which the arms view draws the Galaxy's tracers, millions of years. */
export const ARMS_AGE: AgeCut = 30;

/** The kinds of object the arms view draws: the young tracers of the arms. Cepheids, RR Lyrae
 * stars and globular clusters, older, it turns off. */
export const ARM_TRACERS: readonly TracerKind[] = ["luminous_hot_star", "open_cluster", "star_forming_region"];

/** The kinds of structure the arms view draws: the family of the Galaxy's arms. */
export const ARM_STRUCTURES: readonly string[] = STRUCTURE_FAMILIES.find((f) => f.family === "the Galaxy's arms")?.kinds ?? [];

/** The address's value of `view` for the arms view. */
const ARMS = "arms";

/** Whether the page's address asks for the arms view: `view=arms`. */
export const armsInAddress = (search: string): boolean => new URLSearchParams(search).get("view") === ARMS;

/** The changes that make what is drawn the arms view's, from `hidden`: the Galaxy tracers domain
 * on and every other of `domains` off; the arms' tracers on and the other kinds of object off;
 * the arms on and every other kind of structure, of `structureKinds` or any the explorer knows
 * (the address may open the view before the release's structures are read), off; and every
 * source and kind of value on. */
export function armsChanges(hidden: Hidden, domains: readonly string[], structureKinds: readonly string[]): Change[] {
  const structures = [...new Set([...STRUCTURE_FAMILIES.flatMap((f) => f.kinds), ...structureKinds])];
  const only = (names: readonly string[], on: readonly string[]) => [names.filter((n) => on.includes(n)), names.filter((n) => !on.includes(n))] as const;
  const [tracers, others] = only(domains, [TRACERS_DOMAIN]);
  const [young, old] = only(TRACER_KINDS, ARM_TRACERS);
  const [arms, rest] = only(structures, ARM_STRUCTURES);
  return [
    ...every("domain", tracers, true),
    ...every("domain", others, false),
    ...every("object", young, true),
    ...every("object", old, false),
    ...every("structure", arms, true),
    ...every("structure", rest, false),
    ...every("source", [...hidden.source], true),
    ...every("kind", [...hidden.kind], true),
  ];
}

/** The way to open the arms view in `world`, once it can, with what the Show drawer offers and
 * what it has off in `show`: it sets what is drawn, flies there and puts `view=arms` in the
 * address. An address that asks for it opens it once the release's domains are read; a turn,
 * move or zoom then takes it out of the address, and leaves what is drawn. */
export function useArmsView({ world, show }: { world: World | undefined; show: Pick<ShowPanelProps, "choices" | "hidden" | "onApply" | "age"> }): (() => void) | undefined {
  const controller = world?.controller;
  const { hidden, onApply: apply } = show;
  const { domainNames: domains, structureKinds } = show.choices;
  const chooseAge = show.age?.choose;
  const entered = useRef(false);
  const opened = useRef(false);
  const open = useCallback(() => {
    if (controller === undefined || !controller.armsView()) return;
    apply(armsChanges(hidden, domains, structureKinds));
    chooseAge?.(ARMS_AGE);
    entered.current = true;
    setInAddress({ view: ARMS });
  }, [controller, hidden, apply, chooseAge, domains, structureKinds]);
  // The arms view the address asks for, once, as soon as the domains and their rotations are read.
  useEffect(() => {
    if (opened.current || !domains.includes(TRACERS_DOMAIN) || controller?.canShow("galactic") !== true) return;
    opened.current = true;
    if (armsInAddress(location.search)) open();
  }, [controller, domains, open]);
  // Out of the address at the first move away from it.
  useEffect(() => {
    if (controller === undefined) return undefined;
    return controller.subscribe(() => {
      if (!entered.current || controller.inArmsView) return;
      entered.current = false;
      setInAddress({ view: undefined });
    });
  }, [controller]);
  // Offered only where the release holds the Galaxy's tracers and can turn to galactic axes.
  const offered = domains.includes(TRACERS_DOMAIN) && controller?.canShow("galactic") === true;
  return offered ? open : undefined;
}
