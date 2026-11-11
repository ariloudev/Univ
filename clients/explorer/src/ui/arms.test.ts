import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { manifestOf } from "../test/fixtures.ts";
import { fakeWorld } from "../test/world.ts";
import { ARMS_AGE, ARM_STRUCTURES, ARM_TRACERS, armsChanges, armsInAddress, useArmsView } from "./arms.ts";
import type { AgeCut } from "./age.ts";
import { NOTHING_HIDDEN, applied, hiddenInAddress } from "./hidden.ts";
import type { AgeState } from "./show/AgeChoice.tsx";
import { choicesOf } from "./show/choices.ts";
import type { ShowPanelProps } from "./show/ShowPanel.tsx";
import { createStore } from "./store.ts";

afterEach(() => history.replaceState(null, "", "/"));

const DOMAINS = ["stars", "galaxy_tracers", "nearby_galaxies", "cosmic_web"];

/** What the Show drawer offers and has off: `DOMAINS` (or `domains`) and nothing off, with `parts`. */
function showOf(parts: Partial<Pick<ShowPanelProps, "onApply" | "age">>, domains: readonly string[] = DOMAINS) {
  const choices = { ...choicesOf([], []), domainNames: domains };
  return { choices, hidden: NOTHING_HIDDEN, onApply: () => undefined, ...parts };
}

/** An age choice choosing through `choose`. */
const ageOf = (choose: (cut: AgeCut) => void): AgeState => ({ cut: undefined, choose, held: () => ({ tracers: 0, aged: 0 }), frame: createStore(0) });

describe("what the arms view draws (see-the-galaxys-arms-face-on)", () => {
  it("turns the Galaxy tracers domain on and every other off", () => {
    const hidden = applied(hiddenInAddress("?hide=domain:galaxy_tracers"), armsChanges(hiddenInAddress("?hide=domain:galaxy_tracers"), DOMAINS, []));
    expect([...hidden.domain].sort()).toEqual(["cosmic_web", "nearby_galaxies", "stars"]);
  });

  it("turns the luminous hot stars, open clusters and star-forming regions on, and the Cepheids, RR Lyrae stars and globular clusters off", () => {
    const start = hiddenInAddress("?hide=object:open_cluster");
    const hidden = applied(start, armsChanges(start, DOMAINS, []));
    expect([...hidden.object].sort()).toEqual(["classical_cepheid", "globular_cluster", "rr_lyrae_star"]);
    for (const kind of ARM_TRACERS) expect(hidden.object.has(kind)).toBe(false);
  });

  it("turns the spiral arms and their extrapolations on and every other kind of structure off, the release's own kinds too", () => {
    const start = hiddenInAddress("?hide=structure:spiral_arm");
    const hidden = applied(start, armsChanges(start, DOMAINS, ["void", "bubble"]));
    for (const kind of ARM_STRUCTURES) expect(hidden.structure.has(kind)).toBe(false);
    for (const kind of ["void", "wall", "supercluster", "attractor", "bubble"]) expect(hidden.structure.has(kind)).toBe(true);
  });

  it("turns every source and every kind of value on", () => {
    const start = hiddenInAddress("?hide=source:zari-2021-luminous-hot-stars,source:gaia-dr3-bright,kind:inferred");
    const hidden = applied(start, armsChanges(start, DOMAINS, []));
    expect(hidden.source.size).toBe(0);
    expect(hidden.kind.size).toBe(0);
  });

  it("is asked for by view=arms in the address", () => {
    expect(armsInAddress("?view=arms")).toBe(true);
    expect(armsInAddress("?view=other")).toBe(false);
    expect(armsInAddress("")).toBe(false);
  });
});

describe("opening the arms view", () => {
  // Hoisted: a world made inside renderHook's callback would be made anew on every render.
  const world = fakeWorld();
  world.controller.setRotations(manifestOf("stars").rotations);

  it("sets what is drawn, the age and the view, and puts view=arms in the address; a turn takes it out and leaves what is drawn", () => {
    const apply = vi.fn();
    const chooseAge = vi.fn();
    const { result } = renderHook(() => useArmsView({ world, show: showOf({ onApply: apply, age: ageOf(chooseAge) }) }));
    act(() => result.current?.());
    expect(apply).toHaveBeenCalledWith(armsChanges(NOTHING_HIDDEN, DOMAINS, []));
    expect(chooseAge).toHaveBeenCalledWith(ARMS_AGE);
    expect(world.controller.inArmsView).toBe(true);
    expect(world.controller.view.pitch).toBe(Math.PI / 2);
    expect(new URLSearchParams(location.search).get("view")).toBe("arms");
    act(() => world.controller.turn(0, 30));
    expect(new URLSearchParams(location.search).get("view")).toBeNull();
    expect(apply).toHaveBeenCalledOnce();
  });

  it("takes view=arms out of the address on a zoom or a move of the focus as well", () => {
    const { result } = renderHook(() => useArmsView({ world, show: showOf({}) }));
    for (const move of [() => world.controller.zoom(1), () => world.controller.pan(5, 0)]) {
      act(() => result.current?.());
      expect(new URLSearchParams(location.search).get("view")).toBe("arms");
      act(move);
      expect(new URLSearchParams(location.search).get("view")).toBeNull();
    }
  });

  it("opens from an address that asks for it once the release's domains are read, and not before", () => {
    history.replaceState(null, "", "/?view=arms");
    const opened = fakeWorld();
    opened.controller.setRotations(manifestOf("stars").rotations);
    const apply = vi.fn();
    const { rerender } = renderHook((domains: readonly string[]) => useArmsView({ world: opened, show: showOf({ onApply: apply }, domains) }), {
      initialProps: [] as readonly string[],
    });
    expect(apply).not.toHaveBeenCalled();
    rerender(DOMAINS);
    expect(apply).toHaveBeenCalledOnce();
    expect(opened.controller.inArmsView).toBe(true);
    expect(new URLSearchParams(location.search).get("view")).toBe("arms");
  });
});

describe("where the arms view is offered", () => {
  it("is not offered for a release without the Galaxy's tracers", () => {
    const world = fakeWorld();
    world.controller.setRotations(manifestOf("stars").rotations);
    const { result } = renderHook(() => useArmsView({ world, show: showOf({}, ["stars", "cosmic_web"]) }));
    expect(result.current).toBeUndefined();
  });

  it("ends, and leaves the address, on a change of axes or a flight elsewhere", () => {
    const world = fakeWorld();
    world.controller.setRotations(manifestOf("stars").rotations);
    const { result } = renderHook(() => useArmsView({ world, show: showOf({}) }));
    for (const away of [() => world.controller.setAxes("equatorial"), () => world.controller.flyTo(world.controller.view.focus, world.controller.view.distance)]) {
      act(() => result.current?.());
      expect(new URLSearchParams(location.search).get("view")).toBe("arms");
      act(away);
      expect(world.controller.inArmsView).toBe(false);
      expect(new URLSearchParams(location.search).get("view")).toBeNull();
    }
  });
});
