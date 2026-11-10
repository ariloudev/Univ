// The dust map's switch and what it draws (draw-the-dust-map): off unless the page's address
// turns it on, and kept there while it is; turned on, the release's dust map is fetched (once a
// release) and drawn. Where it cannot be fetched or does not decode, the switch turns itself off
// and says why, and the points drawn stay as they are.

import { useEffect, useState } from "react";

import { DUST_GRID, type DustMap, dustMapOf } from "../api/dust.ts";
import type { Rotations } from "../api/schemas.ts";
import { type AddressSwitch, type Switch, useAddressSwitch } from "./addressSwitch.ts";
import type { World } from "./useWorld.ts";

const DUST: AddressSwitch = { parameter: "dust", byDefault: false };

/** The dust map's switch, the map once drawn, and why it could not be, if it could not. */
export interface DustState {
  state: Switch;
  map: DustMap | undefined;
  problem: string | undefined;
}

const messageOf = (error: unknown): string => (error instanceof Error ? error.message : String(error));

/** The dust map of `release` in `world`, its frames turned by `rotations`; `read` fetches it. */
export function useDustMap(
  release: number,
  world: World | undefined,
  rotations: Rotations | undefined,
  read: (release: number) => Promise<DustMap> = dustMapOf,
): DustState {
  const [on, setOn] = useAddressSwitch(DUST);
  // The map read, with its release: another release's is not drawn while its own is read.
  const [held, setHeld] = useState<{ release: number; map: DustMap }>();
  const [problem, setProblem] = useState<string>();
  useEffect(() => {
    if (!on) return;
    let live = true;
    read(release).then(
      (made) => {
        if (live) setHeld({ release, map: made });
      },
      (error: unknown) => {
        if (!live) return;
        setProblem(`The dust map's grid ${DUST_GRID} could not be drawn: ${messageOf(error)}`);
        setOn(false);
      },
    );
    return () => {
      live = false;
    };
  }, [on, release, read, setOn]);
  const drawn = on && held?.release === release ? held.map : undefined;
  useEffect(() => {
    if (world === undefined || rotations === undefined) return;
    world.renderer.setDust(drawn?.volume, rotations.to_galactic);
    world.controller.invalidate();
  }, [world, rotations, drawn]);
  const turn = (next: boolean) => {
    if (next) setProblem(undefined);
    setOn(next);
  };
  return { state: [on, turn], map: drawn, problem };
}
