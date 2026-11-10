import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DustMap } from "../api/dust.ts";
import type { Rotations } from "../api/schemas.ts";
import { manifestOf } from "../test/fixtures.ts";
import { fakeWorld } from "../test/world.ts";
import { useDustMap } from "./useDustMap.ts";

afterEach(() => history.replaceState(null, "", "/"));

const ROTATIONS: Rotations = manifestOf("stars").rotations;

const MAP: DustMap = {
  volume: { n: 1, corner: -5, edge: 10, densities: new Float32Array([0.01]) },
  title: "Edenhofer et al. 2024 3D dust map",
  citation: "Edenhofer G. et al. 2024, A&A 685, A82",
  credit: "CC BY 4.0",
  unit: "extinction_zgr23_per_parsec",
  kind: "reconstructed",
};

/** A world whose renderer records the dust it is given, built once, outside the hook. */
function world() {
  const made = fakeWorld();
  const setDust = vi.spyOn(made.renderer, "setDust");
  return { made, setDust };
}

describe("the dust map's switch", () => {
  it("is off unless the address turns it on, and reads nothing while off", () => {
    const { made, setDust } = world();
    const read = vi.fn(() => Promise.resolve(MAP));
    const { result } = renderHook(() => useDustMap(13, made, ROTATIONS, read));
    expect(result.current.state[0]).toBe(false);
    expect(read).not.toHaveBeenCalled();
    expect(setDust).toHaveBeenLastCalledWith(undefined, ROTATIONS.to_galactic);
  });

  it("turned on, reads the release's map, draws it placed by the galactic rotation, and is kept on in the address", async () => {
    const { made, setDust } = world();
    const read = vi.fn(() => Promise.resolve(MAP));
    const { result } = renderHook(() => useDustMap(13, made, ROTATIONS, read));
    act(() => result.current.state[1](true));
    await waitFor(() => expect(result.current.map).toBe(MAP));
    expect(read).toHaveBeenCalledWith(13);
    expect(setDust).toHaveBeenLastCalledWith(MAP.volume, ROTATIONS.to_galactic);
    expect(location.search).toBe("?dust=on");
    act(() => result.current.state[1](false));
    expect(setDust).toHaveBeenLastCalledWith(undefined, ROTATIONS.to_galactic);
    expect(location.search).toBe("");
  });

  it("starts on where the address turns it on, as after a reload", async () => {
    history.replaceState(null, "", "/?dust=on");
    const { made } = world();
    const { result } = renderHook(() => useDustMap(13, made, ROTATIONS, () => Promise.resolve(MAP)));
    await waitFor(() => expect(result.current.map).toBe(MAP));
  });

  it("turns itself off where the map cannot be read, naming the grid and the error, and draws no dust", async () => {
    history.replaceState(null, "", "/?dust=on");
    const { made, setDust } = world();
    const read = () => Promise.reject(new Error("Not Found: the release has no grid edenhofer-2024-dust"));
    const { result } = renderHook(() => useDustMap(13, made, ROTATIONS, read));
    await waitFor(() => expect(result.current.state[0]).toBe(false));
    expect(result.current.problem).toContain("edenhofer-2024-dust");
    expect(result.current.problem).toContain("Not Found");
    expect(setDust).not.toHaveBeenCalledWith(expect.anything(), expect.anything());
    // Turned on again, it tries again, the message gone.
    act(() => result.current.state[1](true));
    expect(result.current.problem).toBeUndefined();
  });
});
