import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ANDROMEDA } from "../test/objects.ts";
import { fakeWorld } from "../test/world.ts";
import { objectInAddress, useChosen } from "./useChosen.ts";
import type { World } from "./useWorld.ts";

function world(): World {
  return fakeWorld();
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
    act(() => first.result.current.choose(856254));
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

  it("is read from the address only where it names an object id, with the name it was found by", () => {
    expect(objectInAddress("?object=42")).toEqual({ id: 42, as: undefined });
    expect(objectInAddress("?object=856254&as=Andromeda")).toEqual({ id: 856254, as: "Andromeda" });
    expect(objectInAddress("?object=M31")).toBeUndefined();
    expect(objectInAddress("")).toBeUndefined();
  });

  it("is read once however often it is chosen, and not at all when its record is in hand", async () => {
    const fetch = vi.fn(() => Promise.resolve(Response.json(ANDROMEDA)));
    vi.stubGlobal("fetch", fetch);
    const scene = world();
    const hook = renderHook(() => useChosen(7, scene));
    act(() => hook.result.current.choose(856254));
    act(() => hook.result.current.choose(856254));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("ready"));
    expect(fetch).toHaveBeenCalledTimes(1);
    act(() => hook.result.current.choose(1, { record: { ...ANDROMEDA, id: 1 } }));
    expect(hook.result.current.chosen).toEqual({ kind: "ready", record: { ...ANDROMEDA, id: 1 } });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("is let go of, leaving the address naming none", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    const scene = world();
    const hook = renderHook(() => useChosen(7, scene));
    act(() => hook.result.current.choose(856254));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("ready"));
    act(() => hook.result.current.choose(undefined));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("none"));
    expect(location.search).toBe("");
    expect(scene.chosen).toBeUndefined();
  });

  it("keeps the name it was found by in the address, beside its id", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    const scene = world();
    const hook = renderHook(() => useChosen(7, scene));
    act(() => hook.result.current.choose(856254, { record: ANDROMEDA, as: "Andromeda" }));
    await waitFor(() => expect(hook.result.current.chosen.kind).toBe("ready"));
    expect(new URLSearchParams(location.search).get("as")).toBe("Andromeda");
    expect(hook.result.current.chosen).toMatchObject({ as: "Andromeda" });
    act(() => hook.result.current.choose(undefined));
    expect(location.search).toBe("");
  });

  it("is headed by the name the address gives only where its record bears that name", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    history.replaceState(null, "", "/?object=856254&as=andromeda");
    const first = world();
    const borne = renderHook(() => useChosen(7, first));
    await waitFor(() => expect(borne.result.current.chosen.kind).toBe("ready"));
    expect(borne.result.current.chosen).toMatchObject({ as: "Andromeda" });
    borne.unmount();
    history.replaceState(null, "", "/?object=856254&as=Sirius");
    const second = world();
    const claimed = renderHook(() => useChosen(7, second));
    await waitFor(() => expect(claimed.result.current.chosen.kind).toBe("ready"));
    expect(claimed.result.current.chosen).toMatchObject({ as: undefined });
    expect(location.search).toBe("?object=856254");
  });

  it("takes the name it was found by anew when the same object is found by another name", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(Response.json(ANDROMEDA))));
    const scene = world();
    const hook = renderHook(() => useChosen(7, scene));
    act(() => hook.result.current.choose(856254, { as: "M 31" }));
    await waitFor(() => expect(hook.result.current.chosen).toMatchObject({ as: "M 31" }));
    act(() => hook.result.current.choose(856254, { as: "MESSIER031" }));
    await waitFor(() => expect(hook.result.current.chosen).toMatchObject({ as: "MESSIER031" }));
    // Picked in the view, with no name given: it keeps the one it was found by.
    act(() => hook.result.current.choose(856254));
    expect(hook.result.current.chosen).toMatchObject({ as: "MESSIER031" });
  });
});
