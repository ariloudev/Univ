import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fakeWorld } from "../test/world.ts";
import { outlinesInAddress, useDrawnOutlines } from "./outlinesSwitch.ts";

afterEach(() => history.replaceState(null, "", "/"));

describe("the structures' outlines' switch", () => {
  it("is on unless the address turns the outlines off", () => {
    expect(outlinesInAddress("")).toBe(true);
    expect(outlinesInAddress("?outlines=on")).toBe(true);
    expect(outlinesInAddress("?grid=on&outlines=off")).toBe(false);
  });

  it("is kept off in the address, beside what else it holds, and leaves it once on again, the renderer told each time", () => {
    history.replaceState(null, "", "/?rings=on");
    const world = fakeWorld();
    const showOutlines = vi.spyOn(world.renderer, "showOutlines");
    const { result } = renderHook(() => useDrawnOutlines(world));
    expect(result.current[0]).toBe(true);
    expect(showOutlines).toHaveBeenLastCalledWith(true);
    act(() => result.current[1](false));
    expect(location.search).toBe("?rings=on&outlines=off");
    expect(showOutlines).toHaveBeenLastCalledWith(false);
    act(() => result.current[1](true));
    expect(location.search).toBe("?rings=on");
    expect(showOutlines).toHaveBeenLastCalledWith(true);
  });

  it("starts off where the address turns the outlines off, as after a reload", () => {
    history.replaceState(null, "", "/?outlines=off");
    const { result } = renderHook(() => useDrawnOutlines(undefined));
    expect(result.current[0]).toBe(false);
  });
});
