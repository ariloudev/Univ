import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Controller } from "../scene/controller.ts";
import { ANDROMEDA } from "../test/objects.ts";
import { objectInAddress, useChosen } from "./useChosen.ts";
import type { World } from "./useWorld.ts";

function world(): World {
  const controller = new Controller(
    { draw: () => undefined, setLines: () => undefined },
    { size: () => ({ width: 800, height: 600, ratio: 1 }), now: () => 0, reducedMotion: () => true },
  );
  const renderer = { setMarkers: vi.fn() } as unknown as World["renderer"];
  return { controller, renderer, catalogue: undefined, domains: new Map(), rotations: undefined, pickable: [], chosen: undefined };
}

afterEach(() => {
  vi.unstubAllGlobals();
  history.replaceState(null, "", "/");
});

describe("the chosen object", () => {
  it("is kept in the page's address, read back on reload, marked, and flown to", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    const scene = world();
    const first = renderHook(() => useChosen(7, scene));
    act(() => first.result.current.choose(856254, false));
    await waitFor(() => expect(first.result.current.chosen.kind).toBe("ready"));
    expect(location.search).toBe("?object=856254");
    expect(scene.chosen).toEqual([0.55, 0.1, 0.49]);
    first.unmount();
    // Reloaded: the address names the object, which is read and flown to.
    const reloaded = world();
    const second = renderHook(() => useChosen(7, reloaded));
    await waitFor(() => expect(second.result.current.chosen.kind).toBe("ready"));
    expect(reloaded.controller.view.focus).toEqual([0.55, 0.1, 0.49]);
  });

  it("is read from the address only where it names an object id", () => {
    expect(objectInAddress("?object=42")).toBe(42);
    expect(objectInAddress("?object=M31")).toBeUndefined();
    expect(objectInAddress("")).toBeUndefined();
  });

  it("is let go of, leaving the address naming none", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    const scene = world();
    const hook = renderHook(() => useChosen(7, scene));
    act(() => hook.result.current.choose(856254, false));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("ready"));
    act(() => hook.result.current.choose(undefined, false));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("none"));
    expect(location.search).toBe("");
    expect(scene.chosen).toBeUndefined();
  });
});
